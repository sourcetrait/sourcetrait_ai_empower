use crate::*;

pub(crate) fn base_context() -> nu::EngineState {
    let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
    engine_state = nu::add_extra_command_context(engine_state);
    engine_state.is_interactive = false;
    engine_state.is_mcp = true;
    engine_state
}
