use crate::*;

/// What: the host binary's CLI surface -- the `--id` / `--namespace`,
/// the agent work dir (`--workdir`), the
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
    /// Path to a grammar_mcp.toml, carrying the settings that are NOT
    /// arguments (currently the [channel] table). An absent or malformed
    /// path is a hard error.
    #[arg(long)]
    pub config: Option<String>,
    /// Agent identity owning the state namespace (trusted operator config).
    /// Defaults to the invoking user's name.
    #[arg(long, default_value_t = default_id())]
    pub id: String,
    /// State namespace within the id's identity.
    #[arg(long, default_value = "default")]
    pub namespace: String,
    /// Agent working directory, exported to every eval body as
    /// $env.EQUIP_WORK_DIR. Defaults to <home>/proj/equip/<id>.
    #[arg(long)]
    pub workdir: Option<String>,
    /// Comma-separated tools to deny: run,rerun,interact,call,learn,new,
    /// commit,rig,channel_open,channel_verified,channel_close,config_channel,
    /// purview_list,purview_configure,purview_extend,purview_reset.
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
/// no MCP client -- rig authoring (new/commit/rig), consumption
/// (call/inspect/info), and eval (run) -- through whatever front wraps
/// this binary. Deny does not apply here (it gates agent registration,
/// not the operator).
///
/// Where: dispatched by `server::oneshot::run_oneshot`.
#[derive(clap::Subcommand)]
pub(crate) enum CliTool {
    /// Versions, plugins, and rigs summary.
    Info,
    /// Documentation of one namepath (rig,
    /// rig:module/path, or rig:module/path:function).
    Inspect { namepath: String },
    /// Invoke a committed rig function.
    Call {
        /// The function's namepath: rig:module/path:function.
        namepath: String,
        /// Args as a NUON record (default {}).
        args: Option<String>,
        /// Per-call timeout in milliseconds (default 120000).
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
    /// Evaluate a source-code body on a stateless thread.
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
    /// Evaluate a body on a stateful thread. Single-shot: the session
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
    /// established rigs.
    New { namepaths: Vec<String> },
    /// Validate + promote a rig's source tree into the namespace.
    Commit { rig: String },
    /// Rig administration.
    Rig {
        #[arg(value_enum)]
        action: RigCliAction,
        /// The compound rig name: <author>/<name>.
        rig: String,
        /// The rig's source directory (the "are you sure"
        /// cross-check).
        source_dir: String,
    },
}

/// What: the `cli rig` action, as a clap ValueEnum so an unknown
/// action fails at parse instead of round-tripping to the tool's
/// invalid-action error.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub(crate) enum RigCliAction {
    New,
    Install,
    Check,
    Uninstall,
}

impl RigCliAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Install => "install",
            Self::Check => "check",
            Self::Uninstall => "uninstall",
        }
    }
}

/// Every path out of an argument is expanded here, the same way config paths are, so a
/// `~` or `$VAR` means the same thing whichever surface it arrived on.
fn resolve_work_dir(
    raw: Option<&str>,
    id: &str,
) -> Result<PathBuf, String> {
    match raw {
        Some(s) => expand_path(s),
        None => Ok(default_work_dir(id)),
    }
}

fn parse_deniable(s: &str) -> Result<DeniableTool, String> {
    DeniableTool::from_name(s).ok_or_else(|| {
        format!(
            "unknown tool `{s}`; deniable tools: run, rerun, interact, call, learn, new, \
             commit, rig, channel_open, channel_verified, channel_close, config_channel, \
             purview_list, purview_configure, purview_extend, purview_reset",
        )
    })
}

/// The argument-owned values plus whatever the file contributes. The two surfaces are
/// disjoint, so there is no precedence to resolve between them.
fn resolve_config(cli: HostCli) -> Result<(Config, Option<HostCommand>), String> {
    let file = match &cli.config {
        Some(raw) => Config::read_toml(&expand_path(raw)?)?,
        None => ConfigToml::default(),
    };
    let work_dir = resolve_work_dir(cli.workdir.as_deref(), &cli.id)?;
    let config = Config::from_toml(
        file,
        cli.id,
        cli.namespace,
        work_dir,
        DenySet::new(cli.deny),
    )?;
    Ok((config, cli.command))
}

pub async fn host_main() -> process::ExitCode {
    let cli = HostCli::parse();
    let (config, command) = match resolve_config(cli) {
        Ok(resolved) => resolved,
        Err(reason) => {
            eprintln!("{reason}");
            // 2 is the operator-input exit code the one-shot CLI already uses.
            return process::ExitCode::from(2);
        }
    };
    CONFIG.set(config).expect("CONFIG set once at startup");
    match command {
        None => {
            run_server().await;
            process::ExitCode::SUCCESS
        }
        Some(HostCommand::Cli { tool }) => run_oneshot(tool).await,
    }
}
