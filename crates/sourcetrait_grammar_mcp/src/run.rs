use crate::*;

fn set_cli_env_path(k: &'static str, v: &Option<PathBuf>) {
    let Some(v) = v else { return };
    let v = expand_path(v).expect("Valid ENV value");
    unsafe { std::env::set_var(k, v) }
}

pub(crate) fn set_cli_env(cli: &Cli) {
    let Some(env) = &cli.env else { return };

    set_cli_env_path("XDG_CACHE_HOME", &env.xdg_cache_home);
    set_cli_env_path("XDG_CONFIG_HOME", &env.xdg_config_home);
    set_cli_env_path("XDG_DATA_HOME", &env.xdg_data_home);
    set_cli_env_path("XDG_STATE_HOME", &env.xdg_state_home);

    if let Some(vars) = &env.vars {
        for (var,val) in vars {
            unsafe { std::env::set_var(var, val); }
        }
    }
}

pub fn run_main() -> process::ExitCode {
    let cli = Cli::parse();
    run_with(cli)
}

pub fn run_with(cli: Cli) -> process::ExitCode {
    set_cli_env(&cli);
    run(cli)
}

pub async fn start(cli: Cli) -> GrammarMcpResult<impl GrammarMcpServiceTrait> {
    Ok(GrammarMcpService{}) //todo
}

fn run(cli: Cli) -> process::ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(EVAL_STACK_SIZE)
        .build()
        .expect("tokio runtime");
    runtime.block_on(async { tk::spawn(serve_or_oneshot(cli)).await.expect("host task") })
}

async fn serve_or_oneshot(cli: Cli) -> process::ExitCode {
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
        Some(CliCmd::Cli { tool }) => run_oneshot(tool).await,
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
    raw: Option<&Path>,
    id: &str,
) -> Result<PathBuf, String> {
    match raw {
        Some(s) => expand_path(s),
        None => Ok(default_work_dir(id)),
    }
}

pub(crate) fn parse_deniable(s: &str) -> Result<DeniableTool, String> {
    DeniableTool::from_name(s).ok_or_else(|| {
        format!(
            "unknown tool `{s}`; deniable tools: run, rerun, interact, call, learn, new, \
             commit, rig, channel_open, channel_verified, channel_close, config_channel, \
             purviews, purview_configure, purview_extend, purview, \
             remote_channel_open, remote_channel_close, remote_channels",
        )
    })
}

/// The argument-owned values plus whatever the file contributes.
fn resolve_config(cli: Cli) -> Result<(Config, Option<CliCmd>), String> {
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
