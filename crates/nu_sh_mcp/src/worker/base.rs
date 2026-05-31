use crate::*;

pub(crate) struct WarmBase {
    #[allow(dead_code)]
    pub engine_state: nu::EngineState,
}

impl WarmBase {
    pub(crate) fn new() -> Self {
        Self { engine_state: nu::EngineState::new() }
    }
}
