/// What: filter sets carried over from the retired python rustscan
/// implementation, plus the path-shape predicate distinguishing
/// "example" files from architectural-src files.
///
/// Why: each set excludes specific noise classes the scanner cannot
/// usefully attribute (stdlib container types, Rust keywords used as
/// identifiers, universally-used macros, etc.). Per the_user
/// 2026-06-04 these stay as crate constants this iteration; the lift
/// to `calibration.toml` is deferred.
///
/// Where: consumed by the walker (for path scanning + macro detection)
/// and by the macro-body token cursor.

/// Outer-identifier names skipped when emitting `type_usage` records.
/// stdlib containers + universal generic shells + Rust primitive type
/// names + single-letter type-parameter conventions; none of these are
/// architectural protagonists in any workspace.
pub(crate) const TYPE_USAGE_NOISE_TYPES: &[&str] = &[
    "Vec", "VecDeque", "LinkedList", "BinaryHeap",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet",
    "Box", "Arc", "Rc", "Mutex", "RwLock", "RefCell", "Cell", "Weak",
    "OnceCell", "OnceLock",
    "Option", "Result", "Some", "None", "Ok", "Err",
    "String", "Path", "PathBuf", "Cow", "Pin",
    "OsString", "OsStr", "CString", "CStr",
    "Iter", "IterMut", "IntoIter", "Chain", "Map", "Filter", "Take",
    "Skip", "Zip", "Enumerate", "Peekable",
    "std", "core", "alloc",
    "T", "E", "U", "K", "V",
    "f32", "f64",
    "i8", "i16", "i32", "i64", "i128", "isize",
    "u8", "u16", "u32", "u64", "u128", "usize",
    "char", "bool", "str",
];

/// Rust keywords; never recorded as type-usage idents.
pub(crate) const KEYWORDS: &[&str] = &[
    "impl", "trait", "struct", "enum", "union", "type", "fn", "mod", "macro_rules",
    "for", "where", "dyn", "pub", "use", "as", "const", "static", "unsafe", "extern",
    "async", "move", "ref", "mut", "let", "match", "if", "else", "while", "loop",
    "return", "self", "Self", "crate", "super", "in",
];

/// Macro names skipped when emitting `MacroEntry` records (universally
/// used; not architectural signal).
pub(crate) const NOISE_MACROS: &[&str] = &[
    "vec", "println", "print", "eprintln", "eprint", "format", "write", "writeln",
    "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq",
    "debug_assert_ne", "panic", "todo", "unimplemented", "unreachable", "dbg",
    "matches", "include_str", "include_bytes", "include", "env", "option_env",
    "concat", "stringify", "format_args", "cfg", "line", "column", "file",
    "compile_error", "if_chain", "try", "await",
];

/// Attribute names whose function-form is widely used noise rather than
/// architectural attribute-macro signal.
pub(crate) const NOISE_ATTRS: &[&str] = &[
    "case", "rstest", "expect",
    "rustfmt::skip",
    "serde", "strum", "bitflags", "clap", "arg", "command",
];

/// Attribute names that are inert (built-in / always-present rather
/// than user macros). Recorded as `AttrEntry` but never emitted as a
/// `MacroEntry` of kind `AttrMacro`.
pub(crate) const INERT_ATTRS: &[&str] = &[
    "derive", "cfg", "cfg_attr", "allow", "warn", "deny", "forbid", "doc", "must_use",
    "inline", "repr", "non_exhaustive", "automatically_derived", "no_mangle", "used",
    "link_section", "export_name", "test", "ignore", "should_panic", "bench",
    "global_allocator", "panic_handler", "track_caller", "cold", "target_feature",
    "rustfmt", "clippy", "link", "no_link", "path", "macro_use", "macro_export",
    "proc_macro", "proc_macro_derive", "proc_macro_attribute", "stable", "unstable",
];

pub(crate) fn is_noise_macro(name: &str) -> bool {
    if NOISE_MACROS.contains(&name) {
        return true;
    }
    name.starts_with("assert_")
        || name.starts_with("debug_assert_")
        || name.starts_with("async_assert_")
        || name.starts_with("cfg_")
        || name.starts_with("cfg_not_")
}

pub(crate) fn is_noise_attr(path: &str, base: &str) -> bool {
    NOISE_ATTRS.contains(&path) || NOISE_ATTRS.contains(&base)
}

pub(crate) fn is_inert_attr(base: &str) -> bool {
    INERT_ATTRS.contains(&base)
}

/// True when the path lives under `examples/` at any depth - the
/// heuristic for routing emitted `type_usages` into
/// `example_type_usages` rather than the architectural-src bucket.
/// tests/ and benches/ are NOT example files; they are excluded from
/// the workspace walk entirely upstream in `collect_rs_files` and in
/// `walk::walk_workspace`.
pub(crate) fn is_example_file(rel_path: &str) -> bool {
    let p = rel_path.replace('\\', "/");
    p.contains("/examples/") || p.starts_with("examples/")
}
