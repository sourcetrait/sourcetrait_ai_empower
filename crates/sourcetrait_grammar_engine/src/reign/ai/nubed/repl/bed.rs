use crate::*;
//use crate::reign::ai::*;

pub(crate) struct NubedReplBuilder;

impl NubedReplBuilder {
    pub fn build(self) -> NubedRepl {
        let engine_state = Self::build_context(nu_protocol::engine::EngineState::new());

        NubedRepl {
            engine_state,
        }
    }
}

pub(crate) struct NubedRepl {
    pub(crate) engine_state: nu_protocol::engine::EngineState,
}

impl NubedRepl {
    pub(crate) fn evaluate(&mut self, nu: &str) -> GrammarEngineResult<nuin::ValResult> {
        let mut working_set = nu_protocol::engine::StateWorkingSet::new(&self.engine_state);
        let block = nu_parser::parse(&mut working_set, None, nu.as_bytes(), false);
        self.engine_state.merge_delta(working_set.render())
            .unwrap(); //todo

        let mut stack = nu_protocol::engine::Stack::new().capture_all();
        let mut stack = stack.push_redirection(
            Some(nu_protocol::engine::Redirection::Pipe(
                nu_protocol::OutDest::PipeSeparate,
            )),
            None,
        );
        
        let result = nu_engine::eval_block_with_early_return::<nu_protocol::debugger::WithoutDebug>(
                &self.engine_state, &mut stack, &block, nu_protocol::PipelineData::empty()
            )
            .unwrap(); //todo .map_err(Box::new)?;

        let value = result.body
            .into_value(nu_protocol::Span::unknown())
            .unwrap(); //todo

        let val_result = nuin::ValResult::try_from_nu(value).unwrap();
        Ok(val_result)
    }
}
