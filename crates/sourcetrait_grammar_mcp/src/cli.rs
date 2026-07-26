use crate::*;

/// The host binary's CLI surface; a bare invocation serves MCP over stdio.
#[derive(clap::Parser)]
#[command(version, about = "Nushell engine MCP server")]
pub(crate) struct HostCli {
    /// Path to a grammar_mcp.toml carrying the non-argument settings.
    #[arg(long)]
    pub config: Option<String>,
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
    pub workdir: Option<String>,
    /// Comma-separated tools to withhold from the registered surface.
    #[arg(long, value_delimiter = ',', value_parser = parse_deniable)]
    pub deny: Vec<DeniableTool>,
    #[command(subcommand)]
    pub command: Option<HostCommand>,
}

/// The host's subcommands; absent means serve MCP over stdio.
#[derive(clap::Subcommand)]
pub(crate) enum HostCommand {
    /// One-shot CLI over the tool surface; prints one line of bare compact JSON.
    Cli {
        #[command(subcommand)]
        tool: CliTool,
    },
}

/// One-shot mirrors of the MCP tools, for an operator with no agent.
#[derive(clap::Subcommand)]
pub(crate) enum CliTool {
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

/// `--test` supplies a default namespace, never an override.
fn resolve_namespace(
    namespace: Option<String>,
    test: bool,
) -> String {
    namespace.unwrap_or_else(|| {
        if test {
            TEST_NAMESPACE.to_string()
        } else {
            DEFAULT_NAMESPACE.to_string()
        }
    })
}

/// Expand a work-dir argument the same way a config path is expanded.
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
             purviews, purview_configure, purview_extend, purview",
        )
    })
}

/// The argument-owned values plus whatever the file contributes.
fn resolve_config(cli: HostCli) -> Result<(Config, Option<HostCommand>), String> {
    let file = match &cli.config {
        Some(raw) => Config::read_toml(&expand_path(raw)?)?,
        None => ConfigToml::default(),
    };
    let work_dir = resolve_work_dir(cli.workdir.as_deref(), &cli.id)?;
    let namespace = resolve_namespace(cli.namespace, cli.test);
    let config = Config::from_toml(
        file,
        cli.id,
        namespace,
        work_dir,
        DenySet::new(cli.deny),
        cli.test,
    )?;
    Ok((config, cli.command))
}

/// Build the runtime, then run the host on it.
pub fn host_main() -> process::ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(EVAL_STACK_SIZE)
        .build()
        .expect("tokio runtime");
    runtime.block_on(async { tk::spawn(serve_or_oneshot()).await.expect("host task") })
}

async fn serve_or_oneshot() -> process::ExitCode {
    let cli = HostCli::parse();
    let (config, command) = match resolve_config(cli) {
        Ok(resolved) => resolved,
        Err(reason) => {
            eprintln!("{reason}");
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
