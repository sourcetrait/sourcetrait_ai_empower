use crate::*;

/// What: the host binary's CLI surface -- the runtime store coordinate
/// (`--id` / `--namespace`), the agent work dir (`--workdir`), the
/// operator tool-deny list (`--deny`), and an optional `cli` subcommand
/// (the one-shot tool surface). A bare invocation serves MCP over stdio,
/// so `.mcp.json` entries stay plain commands plus args.
///
/// Why: one binary, variant selected at runtime by trusted operator
/// config. `id` / `namespace` are NOT ident-validated here (a bad value
/// errors naturally downstream); `--workdir` is tilde-expanded but not
/// existence-checked (same trust); `--deny` IS validated (fail-fast on an
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
    /// Agent working directory, exported to every eval body as
    /// $env.EQUIP_WORK_DIR. Defaults to <home>/proj/equip/<id>.
    #[arg(long)]
    pub workdir: Option<String>,
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
    /// One-shot CLI over the tool surface (prints the tool envelope as
    /// bare compact JSON, one line, machine format; exit 1 on an error
    /// envelope).
    Cli {
        #[command(subcommand)]
        tool: CliTool,
    },
}

/// What: one-shot mirrors of the 12 MCP tools. Record-shaped inputs
/// arrive as single-quoted NUON strings (e.g. '{x: 5}'); an omitted args
/// value defaults to the empty record. Output is bare compact JSON --
/// the cli's consumer is a wrapper (e.g. a nu-native front that takes
/// real records and serializes at this argv boundary), never a human
/// eye.
///
/// Why: gives the operator the identical tool surface with no agent and
/// no MCP client -- library authoring (new/commit/library), consumption
/// (call/inspect/info), and eval (run) -- through whatever front wraps
/// this binary. Deny does not apply here (it gates agent registration,
/// not the operator).
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
        /// The nonce returned by a prior run().
        nonce: String,
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
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Install => "install",
            Self::Check => "check",
            Self::Uninstall => "uninstall",
        }
    }
}

fn default_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".to_string())
}

fn resolve_work_dir(raw: Option<&str>, id: &str) -> PathBuf {
    match raw {
        Some("~") => BASE_DIRS.home_dir().to_path_buf(),
        Some(s) => match s.strip_prefix("~/") {
            Some(rest) => BASE_DIRS.home_dir().join(rest),
            None => PathBuf::from(s),
        },
        None => BASE_DIRS.home_dir().join("proj").join("equip").join(id),
    }
}

fn parse_deniable(s: &str) -> Result<DeniableTool, String> {
    DeniableTool::from_name(s).ok_or_else(|| {
        format!(
            "unknown tool `{s}`; deniable tools: run, rerun, interact, call, learn, new, \
             commit, library",
        )
    })
}

pub async fn host_main() -> process::ExitCode {
    let cli = HostCli::parse();
    let work_dir = resolve_work_dir(cli.workdir.as_deref(), &cli.id);
    let config = Config {
        id: cli.id,
        namespace: cli.namespace,
        work_dir,
        deny: DenySet::new(cli.deny),
    };
    CONFIG.set(config).expect("CONFIG set once at startup");
    match cli.command {
        None => {
            run_server().await;
            process::ExitCode::SUCCESS
        }
        Some(HostCommand::Cli { tool }) => run_oneshot(tool).await,
    }
}
