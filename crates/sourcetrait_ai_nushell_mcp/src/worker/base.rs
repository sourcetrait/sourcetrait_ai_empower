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
        let mut engine_state = nu::add_shell_command_context(nu::create_default_context());
        // The nu-cmd-extra family (bits / math-trig / str-case / format / roll /
        // to html / from url / ansi gradient) - pure data/string transforms with
        // no admin or side-effect surface; BOTH worker modes load it (item 17,
        // the_user 2026-06-15).
        engine_state = nu::add_extra_command_context(engine_state);
        // The plugin-management family (plugin add/rm/list/use/stop) lands on the
        // STATEFUL (interact) administrative worker ONLY: run() stays admin-free,
        // and plugin registration's merge_delta only persists on the stateful
        // worker anyway (the stateless per-call clone discards it) (item 17).
        if matches!(mode, Mode::Stateful) {
            engine_state = nu::add_plugin_command_context(engine_state);
        }
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        // Slice 5.7: register plugin decls so agent closures can invoke
        // installed plugins (e.g. `from xlsx`, custom plugin commands).
        // Resolves the canonical `$nu.plugin-path` via `nu_path::nu_config_dir`
        // -- the same logic nu binary uses at startup -- and runs the
        // standard `nu_plugin_engine::load_plugin_file` against the registry
        // file. Missing file / parse failures / individual plugin load errors
        // all log + continue; absent plugins are NOT fatal for the worker.
        load_plugins_best_effort(&mut engine_state);
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

/// What: sets `engine_state.plugin_path` to the canonical registry
/// location (resolved by `plugins::registry_path`, mirroring what
/// `$nu.plugin-path` resolves to), reads + deserializes the registry
/// via `plugins::read_registry`, and registers each plugin's decls
/// into a fresh `StateWorkingSet` via `nu_plugin_engine::load_plugin_file`.
/// Merges the resulting delta back into `engine_state` so plugin decls
/// are visible to subsequent parse + eval. Every step is best-effort:
/// no config dir, no file, parse error, individual plugin load error
/// -- all skip silently; the worker stays usable for non-plugin code.
///
/// Why: the worker is the agent's nushell engine; without plugins
/// loaded, agent closures that invoke `from xlsx`, `query db`, or any
/// other plugin command would parse-fail on unknown decls. Path
/// resolution + read share `crate::plugins` with the host-side
/// `info()` tool so the worker's view and the agent's view of the
/// registry can never drift.
///
/// Where: called once in `WarmBase::new` after `add_shell_command_context`
/// but before `seed_env`. Both `Mode::Stateless` and `Mode::Stateful`
/// workers run this -- plugins load symmetrically per the_user 2026-06-01.
fn load_plugins_best_effort(engine_state: &mut nu::EngineState) {
    let Some(path) = plugins::registry_path() else {
        return;
    };
    engine_state.plugin_path = Some(path.clone().into());
    let Some(contents) = plugins::read_registry() else {
        return;
    };
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    // nu_plugin_engine::load_plugin_file returns the count of plugins that
    // failed to load; per-plugin errors are already routed to stderr via
    // report_shell_error inside the call. Slice 6.0: surface the summary so
    // a partial load is visible to the host operator alongside the
    // pre-printed per-plugin errors.
    let error_count = nu::load_plugin_file(&mut working_set, &contents, None);
    if error_count > 0 {
        eprintln!(
            "nushell_mcp_worker: {error_count} plugin(s) failed to load from {}; see preceding error reports",
            path.display(),
        );
    }
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
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
