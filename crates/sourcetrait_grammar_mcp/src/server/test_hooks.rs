//! Test-only engine decls, compiled only under the `test-hooks` feature.

use crate::*;

#[derive(Clone)]
pub(crate) struct TestHangDecl;

impl nu::Command for TestHangDecl {
    fn name(&self) -> &str {
        "__test_hang"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("__test_hang").category(nu::Category::Custom("test".to_string()))
    }

    fn description(&self) -> &str {
        "TEST HOOK: block uninterruptibly (simulates a hung engine thread)"
    }

    fn run(
        &self,
        _engine_state: &nu::EngineState,
        _stack: &mut nu::Stack,
        _call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        std::thread::sleep(std::time::Duration::from_secs(3600));
        Ok(nu::PipelineData::Empty)
    }
}

#[derive(Clone)]
pub(crate) struct TestPanicDecl;

impl nu::Command for TestPanicDecl {
    fn name(&self) -> &str {
        "__test_panic"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("__test_panic").category(nu::Category::Custom("test".to_string()))
    }

    fn description(&self) -> &str {
        "TEST HOOK: panic the eval thread (exercises catch_unwind; the host survives)"
    }

    fn run(
        &self,
        _engine_state: &nu::EngineState,
        _stack: &mut nu::Stack,
        _call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        panic!("__test_panic: intentional test panic");
    }
}

/// Register the test hooks onto an engine base.
pub(crate) fn register_test_hooks(engine_state: &mut nu::EngineState) {
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    working_set.add_decl(Box::new(TestHangDecl));
    working_set.add_decl(Box::new(TestPanicDecl));
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
}
