use crate::*;

/// What: reusable parsing context that wraps an `EngineState` loaded
/// with the language keywords from `nu_cmd_lang` (def, let, const,
/// use, module, export, ...). Cloned per-file via
/// `engine_state_for_file` to layer a per-file `$env.PWD` without
/// disturbing the base.
///
/// Why: building an EngineState is millisecond-scale; reusing one
/// across every file in a `validate_library_source` walk amortizes
/// that cost. The lang context alone is sufficient for the
/// validator because we never eval -- only parse. The body linter
/// (slice 5) will reuse the same substrate.
///
/// Where: instantiated by `library::validate_library_source` once
/// per invocation; passed by reference into the per-file walkers
/// (`validate_function_file_ast`, `validate_mod_nu_ast`).
pub(crate) struct ParseEngine {
    engine_state: nu::EngineState,
}

impl ParseEngine {
    /// What: constructs a fresh `ParseEngine` with the keyword-only
    /// `nu_cmd_lang` context, `is_interactive = false`, and
    /// `is_mcp = true`. Returns by value; the caller owns the state.
    ///
    /// Why: `is_interactive = false` suppresses banner + reedline
    /// behaviors; `is_mcp = true` routes nu-cli `print` to stderr so
    /// any prints from parser-side code don't corrupt host stdio.
    /// Both flags are safe defaults for any non-REPL nu parsing.
    ///
    /// Where: called once per `validate_library_source` invocation
    /// inside `library::validate_library_source`.
    pub(crate) fn new() -> Self {
        let mut engine_state = nu::create_default_context();
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        Self { engine_state }
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

/// What: convert a byte offset into a 1-based (line, column) pair
/// against the given source. `byte_offset` past EOF clamps to the
/// final line.
///
/// Why: nu_parser surfaces parse errors with byte-span positions, but
/// agent-facing violation reports use line numbers. This helper is
/// the translation layer.
///
/// Where: called by `library::validate_function_file_ast`,
/// `validate_mod_nu_ast`, and `check_mod_nu_pipeline_element`
/// whenever a parser-derived span needs to be reported to the agent.
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

/// What: wrap `source` in `module <wrapper_name> { <source> }\n` so
/// `export def`, `export use`, and `export module` parse cleanly --
/// these are illegal at top-level but legal inside a module body.
/// Returns the wrapped source plus the byte length of the synthesized
/// prefix so callers can translate wrapper-relative span offsets back
/// to source-relative.
///
/// Why: nu_parser doesn't expose a public "parse this as a module
/// body" entrypoint; wrapping is the workaround. The prefix-length
/// return is the bookkeeping every caller needs to keep error spans
/// pointing at user source, not at the synthesized wrapper.
///
/// Where: called by `library::validate_function_file_ast` (for
/// function files) and `library::validate_mod_nu_ast` (for mod.nu
/// files). The slice 4.5 + 4.6 probes confirmed this is the
/// canonical pattern.
pub(crate) fn wrap_as_module(source: &str, wrapper_name: &str) -> (String, usize) {
    let prefix = format!("module {wrapper_name} {{\n");
    let prefix_len = prefix.len();
    let wrapped = format!("{prefix}{source}\n}}\n");
    (wrapped, prefix_len)
}
