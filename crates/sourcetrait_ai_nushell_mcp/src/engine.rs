use crate::*;

/// What: the nushell command context BOTH engine-holders share -- the worker's
/// `WarmBase` (which EVALUATES library + body code) and the host's `ParseEngine`
/// (which VALIDATES library code at commit / library(check), and backs the body
/// lint). Layers `nu_cmd_lang::create_default_context` +
/// `nu_command::add_shell_command_context` + `nu_cmd_extra::add_extra_command_context`,
/// then sets `is_interactive = false` + `is_mcp = true`.
///
/// Why: the validator must parse against AT LEAST the command set the code will
/// run against, or it rejects library source that works. The two drifted exactly
/// there -- the validator carried only lang + shell, so a library body calling
/// nu-cmd-extra (`str snake-case`, `bits and`, `format`) ran fine on the worker
/// but failed commit/check as `ExtraPositional("str ", ...)`, an opaque error
/// naming nothing. One constructor makes that class of drift unrepresentable:
/// a layer added here reaches both sides or neither.
///
/// NOT included, deliberately: `nu_cmd_plugin::add_plugin_command_context` (the
/// `plugin add/rm/list/use/stop` admin family). It lives on the STATEFUL worker
/// only, and a committed call-target ALWAYS runs on a stateless pool worker -- so
/// a library can never legally call it, and validating against it would let code
/// commit that fails at runtime. The stateful worker layers it on top itself.
/// Plugin DECLS (a different thing: the registered plugins' own commands) DO
/// reach both sides, via `plugins::load_plugin_decls`.
///
/// Where: `worker::base::WarmBase::new` and
/// `server::parse_engine::ParseEngine::new_full`.
pub(crate) fn base_context() -> nu::EngineState {
    let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
    // nu-cmd-extra: bits / math-trig / str-case / format / roll / to html /
    // from url / ansi gradient -- pure data + string transforms, no admin or
    // side-effect surface, so both workers load it and both must validate it.
    engine_state = nu::add_extra_command_context(engine_state);
    engine_state.is_interactive = false;
    engine_state.is_mcp = true;
    engine_state
}
