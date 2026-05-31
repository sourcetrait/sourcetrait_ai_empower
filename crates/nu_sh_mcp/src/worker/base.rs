use crate::*;

pub(crate) struct WarmBase {
    pub engine_state: nu::EngineState,
    pub mode: Mode,
}

impl WarmBase {
    pub(crate) fn new(mode: Mode) -> Self {
        let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        // Seed env vars from the inherited OS environment. External command
        // resolution (run-external) requires $env.PWD, and most agent-
        // submitted closures will want HOME + PATH. Without env-conversions
        // (no config file), $env.* are simple string values.
        seed_env(&mut engine_state);
        // Best-effort: detach the controlling terminal so externals that try
        // to open /dev/tty (sudo, ssh, psql) fail fast rather than hang.
        // setsid() returns EPERM if the process is already a session leader.
        let _ = sys::setsid();
        Self { engine_state, mode }
    }
}

fn seed_env(engine_state: &mut nu::EngineState) {
    if let Ok(cwd) = std::env::current_dir() {
        engine_state.add_env_var(
            "PWD".to_string(),
            nu::Value::string(cwd.to_string_lossy().into_owned(), nu::Span::unknown()),
        );
    }
    for (key, val) in std::env::vars() {
        if key == "PWD" {
            continue;
        }
        engine_state.add_env_var(
            key,
            nu::Value::string(val, nu::Span::unknown()),
        );
    }
}
