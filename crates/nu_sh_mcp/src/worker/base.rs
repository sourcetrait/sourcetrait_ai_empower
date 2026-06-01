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
        // Best-effort: detach the controlling terminal so externals that try
        // to open /dev/tty (sudo, ssh, psql) fail fast rather than hang.
        // setsid() returns EPERM if the process is already a session leader.
        let _ = sys::setsid();
        Self { engine_state, mode }
    }
}

/// What: sets `engine_state.plugin_path` to the canonical
/// `<nu_config_dir>/plugin.msgpackz` location (which `$nu.plugin-path`
/// also resolves to), opens that file, deserializes its
/// `PluginRegistryFile` contents, and registers each plugin's decls
/// into a fresh `StateWorkingSet` via `nu_plugin_engine::load_plugin_file`.
/// Merges the resulting delta back into `engine_state` so plugin decls
/// are visible to subsequent parse + eval. Every step is best-effort:
/// no config dir, no file, parse error, individual plugin load error
/// -- all skip silently; the worker stays usable for non-plugin code.
///
/// Why: the worker is the agent's nushell engine; without plugins
/// loaded, agent closures that invoke `from xlsx`, `query db`, or any
/// other plugin command would parse-fail on unknown decls. Mirroring
/// the nu binary's startup load (`nu_cli::read_plugin_file`'s shell
/// minus the migration + reedline-noise paths) gives parity with the
/// agent's local nu shell. is_mcp on the engine_state stays compatible
/// with plugin loading -- the load step doesn't gate on it.
///
/// Where: called once in `WarmBase::new` after `add_shell_command_context`
/// but before `seed_env`. Both `Mode::Stateless` and `Mode::Stateful`
/// workers run this -- plugins load symmetrically per the_user 2026-06-01.
fn load_plugins_best_effort(engine_state: &mut nu::EngineState) {
    let Some(config_dir) = nu::nu_config_dir() else {
        return;
    };
    let path = config_dir.join("plugin.msgpackz");
    engine_state.plugin_path = Some(path.clone().into());
    let Ok(mut file) = fs::File::open(&path) else {
        return;
    };
    let Ok(contents) = nu::PluginRegistryFile::read_from(&mut file, None) else {
        return;
    };
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    let _failures = nu::load_plugin_file(&mut working_set, &contents, None);
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
        if key == "PWD" {
            continue;
        }
        engine_state.add_env_var(
            key,
            nu::Value::string(val, nu::Span::unknown()),
        );
    }
}
