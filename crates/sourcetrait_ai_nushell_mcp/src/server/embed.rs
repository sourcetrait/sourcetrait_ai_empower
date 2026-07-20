// EmbedEngine (Phase 1): in-process nushell evaluation - the host-side
// replacement for the worker's `WarmBase` (base.rs) + `eval_source`
// (request_loop.rs). Built once at host startup, held as `Arc<EngineState>`; each
// stateless eval clones it onto a blocking thread. Lives beside the worker until
// the cutover deletes worker/; unwired for now.
#![allow(dead_code)]
use crate::*;

/// Build an engine base, mirroring the worker's `WarmBase::new` minus the
/// process-boundary concerns. `base_context` (shell + extra, is_interactive=false,
/// is_mcp=true) + plugin decls + `$nu.*` + seeded env + the parse-time
/// `$NU_LIB_DIRS` const. The stateful (interact) base additionally layers the
/// `plugin add/rm/list` admin family. `setsid()` is dropped (the host is a child
/// of the MCP client and cannot detach its own tty); the TLS crypto provider is
/// installed once at host startup, not here.
pub(crate) fn build_base(mode: Mode) -> nu::EngineState {
    let mut engine_state = base_context();
    if matches!(mode, Mode::Stateful) {
        engine_state = nu::add_plugin_command_context(engine_state);
    }
    load_plugin_decls(&mut engine_state);
    engine_state.generate_nu_constant();
    seed_env(&mut engine_state);
    seed_lib_dirs(&mut engine_state);
    engine_state
}

/// Seed the process env into `$env` (externals need `$env.PATH`; bodies read the
/// ambient EQUIP_* trio). PWD is set to the process cwd; NU_LIB_DIRS is excluded
/// so it cannot override the parse-time const set below.
fn seed_env(engine_state: &mut nu::EngineState) {
    if let Ok(cwd) = std::env::current_dir() {
        engine_state.add_env_var(
            "PWD".to_string(),
            nu::Value::string(cwd.to_string_lossy().into_owned(), nu::Span::unknown()),
        );
    }
    for (key, val) in std::env::vars() {
        if key == "PWD" || key == "NU_LIB_DIRS" {
            continue;
        }
        engine_state.add_env_var(key, nu::Value::string(val, nu::Span::unknown()));
    }
}

/// Register the canonical libraries dir as the parse-time `$NU_LIB_DIRS` const, so
/// a body's `use rig/<author>/<library>` resolves against the signed store. The
/// host knows `libraries_dir()` directly (no worker spawn-env handoff).
fn seed_lib_dirs(engine_state: &mut nu::EngineState) {
    set_lib_dirs_const(engine_state, &[libraries_dir()]);
}

/// Evaluate a synthesized source string against `engine_state`, redirecting the
/// eval's external stdout/stderr into `<log_dir>/{stdout,stderr}` (fd 1 is the
/// JSON-RPC channel). Returns the body's terminal value as a friendly JSON value.
/// The caller supplies a fresh clone for a stateless eval, or the persistent
/// engine for interact; `persist` merges the body's `$env`/cd back (interact) and
/// strips the per-call `$env.NONCE`. Port of the worker's `eval_source`, minus the
/// IPC/msgpack hop.
pub(crate) fn eval_in_process(
    engine_state: &mut nu::EngineState,
    log_dir: &std::path::Path,
    source: &str,
    persist: bool,
) -> Result<json::Value, String> {
    let stdout_file = fs::File::create(log_dir.join("stdout"))
        .map_err(|e| format!("open {}/stdout: {e}", log_dir.display()))?;
    let stderr_file = fs::File::create(log_dir.join("stderr"))
        .map_err(|e| format!("open {}/stderr: {e}", log_dir.display()))?;
    let mut stack = nu::Stack::new()
        .stdout_file(stdout_file)
        .stderr_file(stderr_file)
        .capture_all();
    engine_state.set_signals(nu::Signals::new(Arc::new(AtomicBool::new(false))));
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    let block = nu::parse(&mut working_set, None, source.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
        let msgs: Vec<String> = working_set
            .parse_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("parse errors: {}", msgs.join("; ")));
    }
    if !working_set.compile_errors.is_empty() {
        let msgs: Vec<String> = working_set
            .compile_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("compile errors: {}", msgs.join("; ")));
    }
    let delta = working_set.render();
    engine_state
        .merge_delta(delta)
        .map_err(|e| format!("merge_delta: {e}"))?;
    let pipeline = nu::eval_block::<nu::WithoutDebug>(
        engine_state,
        &mut stack,
        &block,
        nu::PipelineData::Empty,
    )
    .map_err(|e| format!("eval: {e}"))?;
    let value = pipeline
        .body
        .into_value(nu::Span::unknown())
        .map_err(|e| format!("into_value: {e}"))?;
    if persist {
        let _ = stack.remove_env_var(engine_state, "NONCE");
        engine_state
            .merge_env(&mut stack)
            .map_err(|e| format!("merge_env: {e}"))?;
    }
    let json_value = nu::JsonValue::from_value(value).map_err(|e| format!("Value to JSON: {e}"))?;
    json::to_value(&json_value).map_err(|e| format!("json value: {e}"))
}
