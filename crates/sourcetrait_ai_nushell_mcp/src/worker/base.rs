use crate::*;

pub(crate) struct WarmBase {
    pub engine_state: nu::EngineState,
    pub mode: Mode,
}

impl WarmBase {
    pub(crate) fn new(mode: Mode) -> Self {
        let mut engine_state = base_context();
        if matches!(mode, Mode::Stateful) {
            engine_state = nu::add_plugin_command_context(engine_state);
        }
        load_plugin_decls(&mut engine_state);
        engine_state.generate_nu_constant();
        seed_env(&mut engine_state);
        seed_lib_dirs(&mut engine_state);
        let _ = sys::setsid();
        Self { engine_state, mode }
    }
}

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

fn seed_lib_dirs(engine_state: &mut nu::EngineState) {
    let Ok(dir) = std::env::var("NUSHELL_MCP_LIBRARIES_DIR") else {
        return;
    };
    set_lib_dirs_const(engine_state, &[std::path::PathBuf::from(dir)]);
}
