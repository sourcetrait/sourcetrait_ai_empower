use crate::*;

/// What: reusable parsing context that wraps a full-shell
/// `EngineState` (`create_default_context` +
/// `add_shell_command_context`, is_interactive=false, is_mcp=true).
/// Cloned per-file via `engine_state_for_file` to layer a per-file
/// `$env.PWD` without disturbing the base.
///
/// Why: building an EngineState is millisecond-scale; reusing one
/// across every file in a `validate_library_source` walk amortizes
/// that cost. The full shell context (not lang-only) is required so
/// the body lint can resolve regex-receiver decls (e.g. `str replace
/// --regex`) as Calls rather than ExternalCalls; see `new_full`'s
/// docstring. The body linter shares the same substrate.
///
/// Where: instantiated by `library::validate_library_source` once
/// per invocation; passed by reference into the per-file walkers
/// (`validate_function_file_ast`, `validate_mod_nu_ast`).
pub(crate) struct ParseEngine {
    engine_state: nu::EngineState,
}

impl ParseEngine {
    /// What: constructs a fresh `ParseEngine` carrying the full shell
    /// command set (`nu_cmd_lang::create_default_context` +
    /// `nu_command::add_shell_command_context`) plus the same
    /// `is_interactive = false` and `is_mcp = true` flags.
    ///
    /// Why: the slice 5.x body linter needs `cd`, `ls`, `str replace`,
    /// `parse`, `find`, `split row`, `split column`, etc. resolved as
    /// `Expr::Call` rather than collapsing to `Expr::ExternalCall`.
    /// Without the full shell context, the parser cannot identify the
    /// regex-receiver decl names (the named `--regex` flag flattens to
    /// a plain ext arg and becomes indistinguishable from a path-shape
    /// positional). Slice 5.0 probe `iter/nushell_mcp/
    /// slice_5_0_probe_findings.md` confirmed this trade-off.
    ///
    /// Where: called once in `server::run::run_server` to construct
    /// the `lint_engine: Arc<ParseEngine>` field on `NuSh`. Per-call
    /// clones for parse are cheap because `EngineState`'s data is
    /// Arc-shared.
    pub(crate) fn new_full() -> Self {
        let mut engine_state = nu::create_default_context();
        engine_state = nu::add_shell_command_context(engine_state);
        engine_state.is_interactive = false;
        engine_state.is_mcp = true;
        Self { engine_state }
    }

    /// What: borrow the underlying `EngineState` so callers can construct
    /// a `StateWorkingSet` against it directly.
    ///
    /// Why: the body linter parses synthetic source that contains no
    /// `use ./...` references, so `engine_state_for_file`'s PWD wrap
    /// would be wasted work; exposing the base state lets the lint walker
    /// share it without cloning.
    ///
    /// Where: called by `server::lint::lint_body` to spin up a working
    /// set for each agent submission.
    pub(crate) fn engine_state(&self) -> &nu::EngineState {
        &self.engine_state
    }

    /// Build a per-file engine state for library validation, with two env
    /// vars layered so module `use`s resolve exactly as they will at serve
    /// time:
    ///
    /// - `$env.PWD` = the file's parent dir, so a RELATIVE `use ./<file>.nu`
    ///   / `export module <name>` / `use ../<sibling>` resolves against the
    ///   source tree (without it, parsing a `mod.nu` standalone produces noisy
    ///   `ModuleNotFound` diagnostics for files that DO exist; slice 4.5
    ///   probe_modnu_parse confirmed).
    /// - `$env.NU_LIB_DIRS` = `[libraries_dir()]` (the canonical store), so a
    ///   BY-NAME `use <sibling-library>` resolves an already-committed sibling
    ///   during validation. This mirrors `worker::base::seed_lib_dirs` (which
    ///   the host passes to the worker at serve time): without it, a
    ///   cross-library `use pelos` fails `ModuleNotFound` at commit even though
    ///   it resolves fine once the library is served. Aligning the validator's
    ///   lib path with the worker's is the point - a library that serves must
    ///   validate. Must be a list Value (the parser reads NU_LIB_DIRS as a
    ///   list). A library's own first commit predates its appearance in the
    ///   store, so a by-name SELF-reference (`use <self>`) still won't resolve
    ///   during that commit - intra-library refs use the relative form, which
    ///   PWD covers.
    ///
    /// Validation-only: the body lint borrows the base via `engine_state()`,
    /// so this env layering never touches lint. Clone is cheap-ish because
    /// EngineState's data shares via Arc internally.
    pub(crate) fn engine_state_for_file(&self, file_parent: &std::path::Path) -> nu::EngineState {
        let mut clone = self.engine_state.clone();
        clone.add_env_var(
            "PWD".to_string(),
            nu::Value::string(
                file_parent.to_string_lossy().into_owned(),
                nu::Span::unknown(),
            ),
        );
        clone.add_env_var(
            "NU_LIB_DIRS".to_string(),
            nu::Value::list(
                vec![nu::Value::string(
                    libraries_dir().to_string_lossy().into_owned(),
                    nu::Span::unknown(),
                )],
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

/// What: wrap `body` in `def __lint_body [args: record<{args_schema}>] {
/// {body} }\n` so `$args.field` references inside the agent's closure
/// resolve cleanly during parse. Returns the wrapped source plus the
/// byte length of the synthesized prefix so callers can translate
/// wrapper-relative span offsets back to body-relative.
///
/// Why: the body lint can't parse a bare closure body because the typed
/// `$args` positional must be in scope; wrapping the body inside a `def`
/// with the agent-supplied `args_schema` mirrors what
/// `template::build_run_source` will eventually emit, so the parser sees
/// the same shape the worker will. The prefix-length return is the
/// bookkeeping every span-translation site needs.
///
/// Where: called by `server::lint::lint_body` immediately before
/// `nu_parser::parse`. Mirrors `wrap_as_module` for the library
/// validator.
pub(crate) fn wrap_as_def_body(body: &str, args_type: &str) -> (String, usize) {
    let prefix = format!("def __lint_body [args: {args_type}] {{\n");
    let prefix_len = prefix.len();
    let wrapped = format!("{prefix}{body}\n}}\n");
    (wrapped, prefix_len)
}
