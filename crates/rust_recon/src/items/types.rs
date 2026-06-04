/// What: top-level facts collected by `rust_recon scan items` across an
/// entire workspace, serialized to `recon_items.json` and consumed by
/// characterize.py as the per-file lex+structure feed.
///
/// Why: the python rustscan implementation it replaces produced flat
/// per-fact lists keyed by file; preserving that wire shape lets the
/// downstream merge stay untouched while the syn-based walker becomes
/// the new source of truth.
///
/// Where: instantiated in `items::workspace::scan_workspace` once per
/// `rust_recon scan items` invocation; serialized to `recon_items.json`
/// at scan completion.
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ItemsFacts {
    pub(crate) tool_version: String,
    pub(crate) files_scanned: usize,
    pub(crate) files_parse_failed: usize,
    pub(crate) impls: Vec<ImplEntry>,
    pub(crate) traits: Vec<TraitEntry>,
    pub(crate) types: Vec<TypeEntry>,
    pub(crate) fns: Vec<FnEntry>,
    pub(crate) mods: Vec<ModEntry>,
    pub(crate) uses: Vec<UseEntry>,
    pub(crate) macros: Vec<MacroEntry>,
    pub(crate) macro_defs: Vec<MacroDefEntry>,
    pub(crate) attrs: Vec<AttrEntry>,
    pub(crate) derives: Vec<DeriveEntry>,
    pub(crate) type_usages: Vec<TypeUsageEntry>,
    pub(crate) example_type_usages: Vec<TypeUsageEntry>,
    pub(crate) seams: std::collections::BTreeMap<String, usize>,
    pub(crate) doc_count: usize,
}

/// What: per-file accumulator the walker fills before contributing into
/// the workspace-level `ItemsFacts`.
///
/// Why: keeping a per-file buffer lets the workspace orchestrator stage
/// syn::parse_file failures without leaving partial state in the
/// aggregate facts; the walker has a single focused mutable target.
///
/// Where: owned by `FileWalker` for the duration of a single file's
/// visit; merged into `ItemsFacts` after the walk completes.
#[derive(Default, Debug)]
pub(crate) struct FileLevelFacts {
    pub(crate) impls: Vec<ImplEntry>,
    pub(crate) traits: Vec<TraitEntry>,
    pub(crate) types: Vec<TypeEntry>,
    pub(crate) fns: Vec<FnEntry>,
    pub(crate) mods: Vec<ModEntry>,
    pub(crate) uses: Vec<UseEntry>,
    pub(crate) macros: Vec<MacroEntry>,
    pub(crate) macro_defs: Vec<MacroDefEntry>,
    pub(crate) attrs: Vec<AttrEntry>,
    pub(crate) derives: Vec<DeriveEntry>,
    pub(crate) type_usages: Vec<TypeUsageEntry>,
    pub(crate) example_type_usages: Vec<TypeUsageEntry>,
    pub(crate) seams: std::collections::HashMap<SeamKind, usize>,
    pub(crate) doc_count: usize,
}

/// What: one impl block seen at item position (`impl X { ... }` or
/// `impl Trait for X { ... }`).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImplEntry {
    pub(crate) file: String,
    #[serde(rename = "trait")]
    pub(crate) trait_name: Option<String>,
    #[serde(rename = "type")]
    pub(crate) type_name: Option<String>,
    pub(crate) line: usize,
    pub(crate) end_line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) cfg: String,
}

/// What: one trait declaration (`trait T { ... }`).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TraitEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

/// What: one type-like declaration: struct, enum, union, or type alias.
/// `kind` distinguishes which shape was declared.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypeEntry {
    pub(crate) file: String,
    pub(crate) kind: TypeEntryKind,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) cfg_gated: bool,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

/// What: which type-like shape a `TypeEntry` records.
///
/// Why: replaces a stringly-typed `kind: String` with a closed enum that
/// serializes via the same wire tokens (`"struct"` / `"enum"` /
/// `"union"` / `"type"`).
///
/// Where: held in every `TypeEntry`; emitted from the four item visitor
/// methods (`visit_item_struct`, `visit_item_enum`, `visit_item_union`,
/// `visit_item_type`) and from in-impl / in-trait associated type
/// emission.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TypeEntryKind {
    Struct,
    Enum,
    Union,
    Type,
}

/// What: one fn declaration at any depth (free, in-impl, in-trait,
/// extern-block); `brace_depth` records nesting at emission time.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct FnEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) brace_depth: usize,
    pub(crate) doc: String,
    pub(crate) visibility: String,
}

/// What: one `mod X` declaration (with or without inline content).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) visibility: String,
}

/// What: one `use X::Y` statement; `reexport` flags `pub use ...`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct UseEntry {
    pub(crate) file: String,
    pub(crate) reexport: bool,
    pub(crate) path: String,
    pub(crate) line: usize,
}

/// What: one macro call site (function-form or attribute-form). `kind`
/// distinguishes the two shapes; `args_count` / `arg_idents` / `brace_depth`
/// populate only for function-form invocations.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MacroEntry {
    pub(crate) file: String,
    pub(crate) kind: MacroEntryKind,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) expansion_unverified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) args_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) arg_idents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) brace_depth: Option<usize>,
}

/// What: which macro-call shape a `MacroEntry` records.
///
/// Why: replaces a stringly-typed `kind: String` with a closed enum
/// that serializes to the same wire tokens (`"macro_invocation"` /
/// `"attr_macro"`).
///
/// Where: held in every `MacroEntry`; `MacroInvocation` for `foo!(...)`
/// shapes and `AttrMacro` for `#[foo]` shapes.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MacroEntryKind {
    MacroInvocation,
    AttrMacro,
}

/// What: one `macro_rules! NAME { ... }` declaration site.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct MacroDefEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) line: usize,
    pub(crate) visibility: String,
    pub(crate) macro_exported: bool,
}

/// What: one attribute occurrence (`#[...]` or `#![...]`). `inner`
/// distinguishes file-level inner attrs from item-level outer attrs;
/// `args` is the literal text inside the attribute's argument list.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct AttrEntry {
    pub(crate) file: String,
    pub(crate) path: String,
    pub(crate) base: String,
    pub(crate) args: String,
    pub(crate) inner: bool,
    pub(crate) line: usize,
}

/// What: one trait name inside a `#[derive(...)]` list (one entry per
/// trait listed, not per derive attribute).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct DeriveEntry {
    pub(crate) file: String,
    #[serde(rename = "trait")]
    pub(crate) trait_name: String,
    pub(crate) line: usize,
}

/// What: one workspace-defined-pub type used at a factory-call /
/// match-arm / pattern site. `name` combines outer and inner segments
/// as `Outer::inner`. `kind_hint` identifies which architectural shape
/// the usage represents (currently only `FactoryCall`).
/// `expansion_unverified` is set when the usage was extracted from a
/// macro body's TokenStream rather than parsed syntax.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypeUsageEntry {
    pub(crate) file: String,
    pub(crate) name: String,
    pub(crate) kind_hint: TypeUsageKind,
    pub(crate) line: usize,
    pub(crate) brace_depth: usize,
    pub(crate) expansion_unverified: bool,
}

/// What: which architectural shape a type usage represents.
///
/// Why: replaces a stringly-typed `kind_hint: String` (whose only
/// emitted value was `"factory_call"`) with a closed enum. Future
/// shapes (`MatchPattern`, `LetPattern`, ...) can be added without
/// breaking the wire schema since the existing variant keeps its
/// canonical token.
///
/// Where: held in every `TypeUsageEntry`; emitted via
/// `FileWalker::record_type_usage` from `visit_expr_call`,
/// `visit_pat_tuple_struct`, and the macro-body token cursor.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TypeUsageKind {
    FactoryCall,
}

/// What: which boundary the scanner cannot statically cross. Tallied
/// per-file and merged into the workspace-level `seams` map.
///
/// Why: the prior implementation used nine flat `seams_X: usize` fields
/// on `FileLevelFacts` and converted them to a `HashMap<String, usize>`
/// at scan_workspace exit; the enum collapses that conversion into the
/// type system, closes the seam set against accidental new keys, and
/// folds `libc` plus `syscall` increments into one variant via the
/// `wire_key` mapping (`SyscallLibc`).
///
/// Where: keys for `FileLevelFacts::seams`; converted to `String`
/// (snake_case wire form) at scan_workspace exit when populating
/// `ItemsFacts::seams`.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) enum SeamKind {
    Extern,
    NoStd,
    ProcessSpawn,
    SyscallLibc,
    SerdeSerialize,
    StdIoStream,
    Unsafe,
}

impl SeamKind {
    /// What: the canonical snake_case key used in the
    /// `recon_items.json` `seams` map.
    ///
    /// Why: serde_json maps require string keys; using a manual
    /// mapping keeps the wire schema explicit and bullet-proof against
    /// future field reorderings.
    ///
    /// Where: called at scan_workspace exit when collapsing per-file
    /// `HashMap<SeamKind, usize>` into the wire `BTreeMap<String, usize>`.
    pub(crate) fn wire_key(self) -> &'static str {
        match self {
            SeamKind::Extern => "extern",
            SeamKind::NoStd => "no_std",
            SeamKind::ProcessSpawn => "process_spawn",
            SeamKind::SyscallLibc => "syscall_libc",
            SeamKind::SerdeSerialize => "serde_serialize",
            SeamKind::StdIoStream => "std_io_stream",
            SeamKind::Unsafe => "unsafe",
        }
    }
}
