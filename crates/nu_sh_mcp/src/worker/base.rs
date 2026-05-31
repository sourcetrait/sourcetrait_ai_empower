use crate::*;

pub(crate) struct WarmBase {
    pub engine_state: nu::EngineState,
}

impl WarmBase {
    pub(crate) fn new() -> Self {
        let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        // best-effort: detach the controlling terminal so externals that try
        // to open /dev/tty (sudo, ssh, psql) fail fast rather than hang.
        // setsid() returns EPERM if the process is already a session leader.
        let _ = sys::setsid();
        Self { engine_state }
    }
}
