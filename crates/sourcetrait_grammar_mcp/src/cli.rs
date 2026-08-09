use crate::*;

/// The host binary's CLI surface; a bare invocation serves MCP over stdio.
#[derive(Default, clap::Parser)]
#[command(version, about = "Grammar MCP")]
pub struct Cli {
    /// Path to a grammar_mcp.toml carrying the non-argument settings.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Agent identity owning the state namespace; defaults to the invoking user.
    #[arg(long, default_value_t = default_id())]
    pub id: String,
    /// State namespace within the id; defaults to `default`, or `test` under --test.
    #[arg(long)]
    pub namespace: Option<String>,
    /// Test channel: the namespace defaults to test; the watchdog follows the channel.
    #[arg(long)]
    pub test: bool,
    /// Agent working directory, exported to bodies as $env.EQUIP_WORK_DIR.
    #[arg(long)]
    pub workdir: Option<PathBuf>,
    /// Comma-separated tools to withhold from the registered surface.
    #[arg(long, value_delimiter = ',', value_parser = parse_deniable)]
    pub deny: Vec<DeniableTool>,
    #[command(subcommand)]
    pub command: Option<CliCmd>,
    #[arg(skip)]
    pub env: Option<CliEnv>,
}

/// Environment overrides
#[derive(Default, Debug, Clone)]
pub struct CliEnv {
    pub xdg_cache_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub vars: Option<HashMap<String, String>>,
}

/// The host's subcommands; absent means serve MCP over stdio.
#[derive(clap::Subcommand)]
pub enum CliCmd {
    /// One-shot CLI over the tool surface; prints one line of bare compact JSON.
    Cli {
        #[command(subcommand)]
        tool: CliTool,
    },
}

/// One-shot mirrors of the MCP tools, for an operator with no agent.
#[derive(clap::Subcommand)]
pub enum CliTool {
    /// Versions, plugins, and rigs summary.
    Info,
    /// Documentation of one namepath, at any arity.
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
    /// Evaluate a body on a stateful thread; single-shot, state dies with it.
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
    /// List in-flight usage; a one-shot invocation shows none.
    Processes,
    /// Cancel an in-flight usage by nonce. Process-scoped.
    Kill { nonce: String },
    /// Generate the /nu skill at <harness_dir>/skills/nu/SKILL.md.
    Learn { harness_dir: String },
    /// Scaffold module / function skeletons by namepath into established rigs.
    New { namepaths: Vec<String> },
    /// Validate + promote a rig's source tree into the namespace.
    Commit { rig: String },
    /// Rig administration.
    Rig {
        #[arg(value_enum)]
        action: RigCliAction,
        /// The compound rig name: <author>/<name>.
        rig: String,
        /// The rig's source directory (the "are you sure" cross-check).
        source_dir: String,
    },
}

/// The `cli rig` action; an unknown one fails at parse.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum RigCliAction {
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
