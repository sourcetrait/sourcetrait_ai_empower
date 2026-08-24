use crate::*;
//use crate::reign::ai::*;

impl NubedReplBuilder {
    pub(crate) fn build_context(mut engine_state: nu_protocol::engine::EngineState) -> nu_protocol::engine::EngineState {
        // todo: customize
        engine_state = nu_command::add_shell_command_context(engine_state);
        engine_state = nu_cmd_extra::add_extra_command_context(engine_state);
        engine_state
    }
}