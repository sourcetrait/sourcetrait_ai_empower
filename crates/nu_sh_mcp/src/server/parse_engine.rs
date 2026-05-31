use crate::*;

/// Reusable parsing context for the import-time validator (and, in
/// slice 5, the body linter). Construction is heavy (a fresh
/// `nu_protocol::EngineState` with `nu_cmd_lang` keywords loaded);
/// callers should build one per `validate_library_source` invocation
/// and reuse it across files.
pub(crate) struct ParseEngine {
    engine_state: nu::EngineState,
}

impl ParseEngine {
    pub(crate) fn new() -> Self {
        let mut engine_state = nu::create_default_context();
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        Self { engine_state }
    }

    pub(crate) fn engine_state(&self) -> &nu::EngineState {
        &self.engine_state
    }

    /// Build a per-file engine state with `$env.PWD` set to the file's
    /// parent directory. nu_parser resolves `export use ./<file>.nu`
    /// and `export module <name>` relative to `$env.PWD`; without this
    /// guard, parsing a `mod.nu` standalone produces noisy
    /// `ModuleNotFound` diagnostics for files that DO exist on disk
    /// (slice 4.5 experiment: probe_modnu_parse confirmed). Clone is
    /// cheap-ish because EngineState's data shares via Arc internally.
    pub(crate) fn engine_state_for_file(
        &self,
        file_parent: &std::path::Path,
    ) -> nu::EngineState {
        let mut clone = self.engine_state.clone();
        clone.add_env_var(
            "PWD".to_string(),
            nu::Value::string(
                file_parent.to_string_lossy().into_owned(),
                nu::Span::unknown(),
            ),
        );
        clone
    }
}

/// Convert a byte offset into a 1-based (line, column) pair against
/// the given source. Used to translate parser spans back into line
/// numbers the agent can act on. `byte_offset` past EOF clamps to the
/// final line.
pub(crate) fn span_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, b) in source.bytes().enumerate() {
        if i >= byte_offset {
            return (line, col);
        }
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Wrap `source` in `module <wrapper_name> { <source> }` so `export def`
/// and `export use` / `export module` are valid at the body level, and
/// return the byte length of the synthesized prefix (so callers can
/// translate wrapper-relative span offsets back to source-relative).
pub(crate) fn wrap_as_module(source: &str, wrapper_name: &str) -> (String, usize) {
    let prefix = format!("module {wrapper_name} {{\n");
    let prefix_len = prefix.len();
    let wrapped = format!("{prefix}{source}\n}}\n");
    (wrapped, prefix_len)
}
