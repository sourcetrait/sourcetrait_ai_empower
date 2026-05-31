use crate::*;

/// What: the worker process's persistent engine state plus its
/// execution mode. Built once per worker startup; eval_source reads
/// from it on every RunRequest.
///
/// Why: keeping a warm EngineState (already loaded with shell command
/// context + env vars + signals config) lets every call skip the
/// startup cost. The `mode` field is what tells `eval_source` whether
/// to clone the state per-call (stateless) or mutate it in place
/// (stateful).
///
/// Where: constructed once by `worker::run::run_worker` after
/// reading `--mode` from the CLI; passed by `&mut` into
/// `worker::request_loop::serve` which holds it for the worker's
/// lifetime.
pub(crate) struct WarmBase {
    pub engine_state: nu::EngineState,
    pub mode: Mode,
}

impl WarmBase {
    /// What: constructs a `WarmBase` with the full shell command
    /// context loaded (nu_cmd_lang + nu_command), `is_interactive =
    /// false`, `is_mcp = true`, seeded env from the OS environment,
    /// and a best-effort `setsid()` to detach the controlling
    /// terminal. Returns by value; the caller owns it.
    ///
    /// Why: the engine state needs the SHELL command set (not just
    /// keywords) because worker eval can run external commands and
    /// builtins; seeding env makes `$env.HOME`/`$env.PATH` available
    /// for run-external; `setsid()` prevents commands like `sudo`/
    /// `ssh` from hanging on /dev/tty by detaching the worker from
    /// the controlling terminal.
    ///
    /// Where: called once at worker startup by
    /// `worker::run::run_worker` before the request loop starts.
    pub(crate) fn new(mode: Mode) -> Self {
        let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        // Seed env vars from the inherited OS environment. External command
        // resolution (run-external) requires $env.PWD, and most agent-
        // submitted closures will want HOME + PATH. Without env-conversions
        // (no config file), $env.* are simple string values.
        seed_env(&mut engine_state);
        // Best-effort: detach the controlling terminal so externals that try
        // to open /dev/tty (sudo, ssh, psql) fail fast rather than hang.
        // setsid() returns EPERM if the process is already a session leader.
        let _ = sys::setsid();
        Self { engine_state, mode }
    }
}

/// What: copies env vars from the OS environment into the
/// EngineState's `$env`. PWD is special-cased first to ensure it's
/// set to the current working dir (not whatever inherited PWD might
/// be); all other env vars are forwarded as string Values.
///
/// Why: agent closures often read `$env.HOME`, `$env.PATH`,
/// `$env.PWD`; running with `--no-config-file` means env-conversions
/// aren't applied, so all values are simple strings. Without this,
/// run-external commands would fail at PATH lookup.
///
/// Where: called once inside `WarmBase::new` during worker startup.
fn seed_env(engine_state: &mut nu::EngineState) {
    if let Ok(cwd) = std::env::current_dir() {
        engine_state.add_env_var(
            "PWD".to_string(),
            nu::Value::string(cwd.to_string_lossy().into_owned(), nu::Span::unknown()),
        );
    }
    for (key, val) in std::env::vars() {
        if key == "PWD" {
            continue;
        }
        engine_state.add_env_var(
            key,
            nu::Value::string(val, nu::Span::unknown()),
        );
    }
}
