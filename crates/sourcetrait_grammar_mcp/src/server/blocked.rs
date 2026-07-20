use crate::*;

/// A shadow command that replaces a host-fatal builtin (`exit`, `exec`, `panic`)
/// in the in-process EmbedEngine: it parses like the real one (a catch-all `rest`)
/// but always errors at runtime, so a body invoking it fails that single eval
/// instead of terminating the shared host process.
///
/// EmbedEngine principle 1 (intercept every host-death vector): `exit` calls
/// `std::process::exit` and `exec` replaces the process image - either would take
/// the whole MCP down in-process, where there is no worker subprocess boundary to
/// absorb it (the boundary the shell-out era relied on).
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

/// The host-fatal builtins shadowed out of every in-process engine base:
/// `exit` -> `std::process::exit`, `exec` -> `execvp` image replacement, `panic`
/// -> an unconditional `panic!` (the nu-command Debug builtin whose whole purpose
/// is to crash nushell; catch_unwind catches it but it poisons shared mutexes on
/// the way up, so shadowing it out is cleaner). A full nu 0.114.1 registered-decl
/// audit found these three the only builtins that terminate the host DIRECTLY
/// in-process from user input. The guardrail principle: shadow a vector whose
/// LOCAL intent (tear down this eval's engine state) would be a GLOBAL host
/// teardown; a command with explicit target intent stays ALLOWED. So `kill <pid>`
/// is NOT shadowed - it names a process and means it (killing the host's own pid is
/// the operator's intent; the body runs as the assigned box user with those rights
/// and could signal any pid via an external regardless). The shadows guard SURPRISE
/// self-teardown, not a trusted body's deliberate box actions.
pub(crate) const HOST_FATAL_DECLS: &[&str] = &["exit", "exec", "panic"];

/// Shadow every host-fatal builtin with an erroring decl on `engine_state`, so a
/// run()/interact() body can never call one and take down the shared host. Must
/// run after the shell command context that defines the real ones, so the shadow
/// wins name resolution (a later-registered decl overrides the earlier).
pub(crate) fn shadow_host_fatal_decls(engine_state: &mut nu::EngineState) {
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    for name in HOST_FATAL_DECLS {
        working_set.add_decl(Box::new(BlockedDecl::new(name)));
    }
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
}
