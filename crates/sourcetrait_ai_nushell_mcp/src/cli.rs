use crate::*;

/// What: the host binary's CLI surface -- the runtime store coordinate
/// (`--id` / `--namespace`), the operator tool-deny list (`--deny`), and
/// an optional `cli` subcommand (the human one-shot surface). A bare
/// invocation serves MCP over stdio, so `.mcp.json` entries stay plain
/// commands plus args.
///
/// Why: one binary, variant selected at runtime by trusted operator
/// config. `id` / `namespace` are NOT ident-validated here (a bad value
/// errors naturally downstream); `--deny` IS validated (fail-fast on an
/// unknown tool name -- a typo silently denying nothing would defeat the
/// operator's intent).
///
/// Where: parsed by `host_main` from `src/main.rs`.
#[derive(clap::Parser)]
#[command(version, about = "Nushell engine MCP server")]
pub(crate) struct HostCli {
    /// Agent identity owning the state store (trusted operator config).
    /// Defaults to the invoking user's name.
    #[arg(long, default_value_t = default_id())]
    pub id: String,
    /// State namespace within the id's store.
    #[arg(long, default_value = "default")]
    pub namespace: String,
    /// Comma-separated tools to deny:
    /// run,rerun,interact,call,learn,new,commit,library.
    #[arg(long, value_delimiter = ',', value_parser = parse_deniable)]
    pub deny: Vec<DeniableTool>,
    #[command(subcommand)]
    pub command: Option<HostCommand>,
}

/// What: the host's subcommands. Absent = serve MCP over stdio.
#[derive(clap::Subcommand)]
pub(crate) enum HostCommand {
    /// One-shot human CLI over the tool surface (prints the tool
    /// envelope as pretty NUON; exit 1 on an error envelope).
    Cli {
        #[command(subcommand)]
        tool: CliTool,
    },
}

/// What: one-shot mirrors of the 12 MCP tools. Record-shaped inputs
/// arrive as single-quoted NUON strings (e.g. '{x: 5}'); an omitted args
/// value defaults to the empty record.
///
/// Why: gives the operator the identical tool surface with no agent and
/// no MCP client -- library authoring (new/commit/library), consumption
/// (call/inspect/info), and eval (run) from any terminal. Deny does not
/// apply here (it gates agent registration, not the human).
///
/// Where: dispatched by `server::oneshot::run_oneshot`.
#[derive(clap::Subcommand)]
pub(crate) enum CliTool {
    /// Versions, plugins, and libraries summary.
    Info,
    /// Documentation of one node by namepath (library,
    /// library:module/path, or library:module/path:function).
    Inspect { namepath: String },
    /// Invoke a committed library function.
    Call {
        /// The function's namepath: library:module/path:function.
        namepath: String,
        /// Args as a NUON record (default {}).
        args: Option<String>,
        /// Per-call timeout in milliseconds (default 120000).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Evaluate a source-code body on a stateless worker.
    Run {
        /// The nushell source-code body.
        body: String,
        /// Args schema as a NUON record of field -> type name (default {}).
        #[arg(long)]
        args_schema: Option<String>,
        /// Result schema as a NUON record (default {}).
        #[arg(long)]
        result_schema: Option<String>,
        /// Args as a NUON record (default {}).
        #[arg(long)]
        args: Option<String>,
        /// Per-call timeout in milliseconds (default 120000).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Evaluate a body on a stateful worker. Single-shot: the session
    /// state dies with this process.
    Interact {
        /// The nushell source-code body.
        body: String,
        /// Args schema as a NUON record of field -> type name (default {}).
        #[arg(long)]
        args_schema: Option<String>,
        /// Result schema as a NUON record (default {}).
        #[arg(long)]
        result_schema: Option<String>,
        /// Args as a NUON record (default {}).
        #[arg(long)]
        args: Option<String>,
        /// Per-call timeout in milliseconds (default 120000).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Re-evaluate a cached run() body with fresh args.
    Rerun {
        /// The id returned by a prior run().
        rerun_id: String,
        /// Args as a NUON record (default {}).
        args: Option<String>,
        /// Per-call timeout in milliseconds (default 120000).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// List in-flight usage. Process-scoped: a one-shot invocation
    /// shows none.
    Processes,
    /// Cancel an in-flight usage by nonce. Process-scoped.
    Kill { nonce: String },
    /// Generate the /nu skill at <harness_dir>/skills/nu/SKILL.md.
    Learn { harness_dir: String },
    /// Scaffold module / function skeletons by namepath into
    /// established libraries.
    New { namepaths: Vec<String> },
    /// Validate + promote a library's source tree into the store.
    Commit { library: String },
    /// Library administration.
    Library {
        #[arg(value_enum)]
        action: LibraryCliAction,
        /// The compound library coordinate: <author>/<name>.
        library: String,
        /// The library's source directory (the "are you sure"
        /// cross-check).
        source_dir: String,
    },
}

/// What: the `cli library` action, as a clap ValueEnum so an unknown
/// action fails at parse instead of round-tripping to the tool's
/// invalid-action error.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub(crate) enum LibraryCliAction {
    New,
    Install,
    Check,
    Uninstall,
}

impl LibraryCliAction {
    /// The action string the library() tool takes.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Install => "install",
            Self::Check => "check",
            Self::Uninstall => "uninstall",
        }
    }
}

/// Default `--id`: the invoking user's name ($USER), or "default" when
/// the var is absent. A one-time process read for the zero-config human
/// case, not a configuration channel -- harness configs always pass
/// --id explicitly.
fn default_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".to_string())
}

/// clap value_parser for `--deny` tokens; rejects unknown names with the
/// valid-token list so a typo fails the startup instead of silently
/// denying nothing.
fn parse_deniable(s: &str) -> Result<DeniableTool, String> {
    DeniableTool::from_name(s).ok_or_else(|| {
        format!(
            "unknown tool `{s}`; deniable tools: run, rerun, interact, call, learn, new, \
             commit, library",
        )
    })
}

/// What: the host binary's entry point: parse `HostCli`, store the
/// runtime `Config` (the set-once global every host-side reader uses),
/// then either serve MCP over stdio (no subcommand) or run the one-shot
/// human CLI.
///
/// Why: CONFIG is stored here -- before serve/one-shot dispatch -- so it
/// precedes every reader (path helpers, router assembly, worker spawns)
/// on both paths.
///
/// Where: called from `src/main.rs::main`.
pub fn host_main() {
    let cli = HostCli::parse();
    let config = Config {
        id: cli.id,
        namespace: cli.namespace,
        deny: DenySet::new(cli.deny),
    };
    CONFIG.set(config).expect("CONFIG set once at startup");
    match cli.command {
        None => run_server(),
        Some(HostCommand::Cli { tool }) => run_oneshot(tool),
    }
}

/// What: clap-derived enum mirroring the worker binary's `--mode` CLI
/// flag values. Translated to the lib's `Mode` at the seam in
/// `parse_worker_mode`.
///
/// Why: the host passes one of `stateless` / `stateful` on the worker
/// spawn; clap's `ValueEnum` derive handles parsing the string into
/// the enum. Keeping a CLI-shaped enum separate from the lib's `Mode`
/// enum lets `Mode` stay clap-agnostic.
///
/// Where: parsed via `WorkerCli` in `parse_worker_mode`; mapped to
/// `Mode::Stateless` / `Mode::Stateful` immediately after parse.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub(crate) enum CliMode {
    Stateless,
    Stateful,
}

/// What: clap-derived struct for the worker binary's CLI surface. One
/// required `--mode` flag whose value is parsed as `CliMode`.
///
/// Why: the host spawns the worker with `--mode stateless|stateful`;
/// the worker reads the flag via `WorkerCli::parse()` and translates
/// to `Mode` at the seam.
///
/// Where: constructed via `WorkerCli::parse()` in `parse_worker_mode`.
/// Both `worker_main` and any test that wraps the worker binary route
/// through this parser.
#[derive(clap::Parser)]
#[command(version, about = "nushell_mcp worker subprocess")]
pub(crate) struct WorkerCli {
    #[arg(long)]
    pub mode: CliMode,
}

/// What: parses the worker process's CLI args via `WorkerCli::parse()`
/// and returns the resulting lib-side `Mode`. Exits the process via
/// clap's standard error path if the args are malformed.
///
/// Why: factors the clap-specific parsing out of `worker_main` so the
/// lib's worker entry stays a one-liner + the CLI surface lives in a
/// dedicated cli module. `Parser` is in scope via the lib.rs prelude
/// (`pub(crate) use clap::Parser`); the `clap::Parser` /
/// `clap::ValueEnum` derives reach the macros by path.
///
/// Where: called by `worker::run::worker_main` once per worker process
/// at startup.
pub(crate) fn parse_worker_mode() -> Mode {
    let p = WorkerCli::parse();
    match p.mode {
        CliMode::Stateless => Mode::Stateless,
        CliMode::Stateful => Mode::Stateful,
    }
}
