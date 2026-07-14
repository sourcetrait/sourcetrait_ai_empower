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
    /// context loaded (nu_cmd_lang + nu_command + nu_cmd_extra; plus
    /// nu_cmd_plugin on the Stateful/admin worker), `is_interactive =
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
        // The command context shared with the library validator (crate::engine):
        // lang + shell + nu-cmd-extra, is_interactive = false, is_mcp = true.
        // Anything a library body may legally call has to parse on BOTH, so the
        // layering lives in one constructor.
        let mut engine_state = base_context();
        // The plugin-management family (plugin add/rm/list/use/stop) lands on the
        // STATEFUL (interact) administrative worker ONLY: run() stays admin-free,
        // and plugin registration's merge_delta only persists on the stateful
        // worker anyway (the stateless per-call clone discards it) (item 17).
        // Deliberately NOT in base_context: a call-target always runs stateless,
        // so a library must never be validated against this family.
        if matches!(mode, Mode::Stateful) {
            engine_state = nu::add_plugin_command_context(engine_state);
        }
        // Register plugin decls so agent closures can invoke installed plugins
        // (e.g. `from xlsx`, custom plugin commands). Shared with the validator
        // (crate::plugins) so what serves is what validates; best-effort, an
        // absent/broken registry is not fatal.
        load_plugin_decls(&mut engine_state);
        // Populate the `$nu` const so closures can reference `$nu.plugin-path`,
        // `$nu.home-dir`, etc. Without this every `$nu.*` access surfaces
        // `Variable not found`. Each nu host (REPL, LSP, etc.) calls this
        // manually; nu-protocol doesn't auto-run it. Must come AFTER
        // `plugin_path` is set so the captured `$nu.plugin-path` reflects
        // our resolved location.
        engine_state.generate_nu_constant();
        // Seed env vars from the inherited OS environment. External command
        // resolution (run-external) requires $env.PWD, and most agent-
        // submitted closures will want HOME + PATH. Without env-conversions
        // (no config file), $env.* are simple string values.
        seed_env(&mut engine_state);
        // Set $env.NU_LIB_DIRS to the canonical libraries root the host passed
        // (via NUSHELL_MCP_LIBRARIES_DIR) so run()/interact() bodies can
        // `use <library> <module> ...` against committed libraries.
        seed_lib_dirs(&mut engine_state);
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
        // PWD is set above; NU_LIB_DIRS is deliberately NOT inherited from the
        // box env - the MCP's own store is the sole, const-set lib path (see
        // seed_lib_dirs). Letting the box's value merge in would widen module
        // resolution beyond the controlled store.
        if key == "PWD" || key == "NU_LIB_DIRS" {
            continue;
        }
        engine_state.add_env_var(key, nu::Value::string(val, nu::Span::unknown()));
    }
}

/// What: registers the parse-time CONST `$NU_LIB_DIRS` = `[<libraries root>]`,
/// read from the `NUSHELL_MCP_LIBRARIES_DIR` env var the host sets on the worker
/// spawn. No-op when the var is absent (e.g. a worker spawned outside the host
/// path, like a bare handshake test).
///
/// Why: `use <author>/<library> ...` from a run()/interact() body resolves a
/// module by searching `$NU_LIB_DIRS`. Every committed library is an
/// `<author>/<library>` subtree of the canonical libraries root, so one entry
/// makes them all importable and stays correct as libraries are added/removed.
/// The CONST (not the deprecated `$env.NU_LIB_DIRS` form) is what nu-parser's
/// `find_in_dirs_with_id` reads FIRST - and, being a const, it can't be
/// overridden by an agent body mutating `$env.NU_LIB_DIRS`, so the MCP's store
/// stays the sole controlled lib path. The worker can't resolve the
/// store-namespaced path itself (it never reads `config()`), so the host
/// passes it across the spawn boundary.
///
/// Where: called once in `WarmBase::new` after `seed_env`, both worker modes.
fn seed_lib_dirs(engine_state: &mut nu::EngineState) {
    let Ok(dir) = std::env::var("NUSHELL_MCP_LIBRARIES_DIR") else {
        return;
    };
    set_lib_dirs_const(engine_state, &[std::path::PathBuf::from(dir)]);
}
