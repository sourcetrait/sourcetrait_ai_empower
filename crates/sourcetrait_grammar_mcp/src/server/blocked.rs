use crate::*;

/// A shadow decl replacing a host-fatal builtin; it always errors at runtime.
#[derive(Clone)]
pub(crate) struct BlockedDecl {
    name: &'static str,
}

impl BlockedDecl {
    pub(crate) fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl nu::Command for BlockedDecl {
    fn name(&self) -> &str {
        self.name
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build(self.name)
            .rest("args", nu::SyntaxShape::Any, "ignored; the command always errors")
            .category(nu::Category::System)
    }

    fn description(&self) -> &str {
        "disabled in the grammar in-process engine (would terminate the host)"
    }

    fn run(
        &self,
        _engine_state: &nu::EngineState,
        _stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        Err(nu::GenericError::new(
            format!("`{}` is disabled", self.name),
            format!(
                "`{}` would terminate the grammar host process and is not available in-process",
                self.name,
            ),
            call.head,
        )
        .into())
    }
}

/// The host-fatal builtins shadowed out of every in-process engine base.
pub(crate) const HOST_FATAL_DECLS: &[&str] = &["exit", "exec", "panic"];

/// Shadow every host-fatal builtin with an erroring decl on `engine_state`.
pub(crate) fn shadow_host_fatal_decls(engine_state: &mut nu::EngineState) {
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    for name in HOST_FATAL_DECLS {
        working_set.add_decl(Box::new(BlockedDecl::new(name)));
    }
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
}
