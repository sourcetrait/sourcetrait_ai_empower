use crate::*;

/// What: top-level facts collected by `know_rust scan items` across an
/// entire workspace, serialized to `know_rust_items.json` and consumed
/// by characterize.py as the per-file lex+structure feed.
///
/// Why: the python rustscan implementation it replaces produced flat
/// per-fact lists keyed by file; preserving that wire shape lets the
/// downstream merge stay untouched while the syn-based walker becomes
/// the new source of truth.
///
/// Where: instantiated in `scan::items::workspace::scan_workspace`
/// once per `know_rust scan items` invocation; serialized to
/// `know_rust_items.json` by `scan::items::run::scan_items`.
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct ItemFacts {
    pub tool_version: String,
    pub files_scanned: usize,
    pub files_parse_failed: usize,
    pub impls: Vec<ImplEntry>,
    pub traits: Vec<TraitEntry>,
    pub types: Vec<TypeEntry>,
    pub fns: Vec<FnEntry>,
    pub mods: Vec<ModEntry>,
    pub uses: Vec<UseEntry>,
    pub macros: Vec<MacroEntry>,
    pub macro_defs: Vec<MacroDefEntry>,
    pub attrs: Vec<AttrEntry>,
    pub derives: Vec<DeriveEntry>,
    pub type_usages: Vec<TypeUsageEntry>,
    pub example_type_usages: Vec<TypeUsageEntry>,
    pub seams: BTreeMap<String, usize>,
    pub doc_count: usize,
    /// What: per-picked-pattern carry map. Key is the `Pattern::Display`
    /// form (`<group_wire>:<name>` e.g. `structure:Component`); value is
    /// the list of one-hop dependent names the reader needs to make
    /// sense of the pick.
    ///
    /// Why: refactor phase R2 (per
    /// `notes/know_rust/tasks/picks-data-model-refactor.md`). Carry is
    /// extracted at scan time so downstream consumers see a typed
    /// transitive context layer alongside the existing facts.
    ///
    /// Where: aggregated from per-file
    /// `FileLevelFacts::carries` at `scan_workspace` exit; consumed by
    /// the future picker + emit phases (R3 + R5).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub carries: BTreeMap<Pattern, Vec<CarryEntry>>,
}

/// What: per-file accumulator the walker fills before contributing into
/// the workspace-level `ItemFacts`.
///
/// Why: keeping a per-file buffer lets the workspace orchestrator stage
/// syn::parse_file failures without leaving partial state in the
/// aggregate facts; the walker has a single focused mutable target.
///
/// Where: owned by `FileWalker` for the duration of a single file's
/// visit; merged into `ItemFacts` after the walk completes.
#[derive(Default, Debug)]
pub struct FileLevelFacts {
    pub impls: Vec<ImplEntry>,
    pub traits: Vec<TraitEntry>,
    pub types: Vec<TypeEntry>,
    pub fns: Vec<FnEntry>,
    pub mods: Vec<ModEntry>,
    pub uses: Vec<UseEntry>,
    pub macros: Vec<MacroEntry>,
    pub macro_defs: Vec<MacroDefEntry>,
    pub attrs: Vec<AttrEntry>,
    pub derives: Vec<DeriveEntry>,
    pub type_usages: Vec<TypeUsageEntry>,
    pub example_type_usages: Vec<TypeUsageEntry>,
    pub seams: HashMap<SeamKind, usize>,
    pub doc_count: usize,
    pub carries: HashMap<Pattern, Vec<CarryEntry>>,
}

/// What: one carry entry - a dependent name the walker surfaced as
/// one-hop transitive context for a Picked item. Currently records
/// just the name; future R3 enrichment may add `group: PickGroup`
/// when the walker can disambiguate.
///
/// Why: refactor phase R2 - the carry concept from
/// `notes/know_rust/picks-data-model.md`. Each picked item's
/// reader-context comes from a list of these.
///
/// Where: held inside `FileLevelFacts::carries` and `ItemFacts::carries`
/// keyed by the picked pattern's `Pattern::Display` form.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CarryEntry {
    pub name: String,
}

/// What: one impl block seen at item position (`impl X { ... }` or
/// `impl Trait for X { ... }`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImplEntry {
    pub file: String,
    #[serde(rename = "trait")]
    pub trait_name: Option<String>,
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub line: usize,
    pub end_line: usize,
    pub cfg_gated: bool,
    pub cfg: String,
}

/// What: one trait declaration (`trait T { ... }`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TraitEntry {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub cfg_gated: bool,
    pub doc: String,
    pub visibility: String,
}

/// What: one type-like declaration: struct, enum, union, or type alias.
/// `kind` distinguishes which shape was declared.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeEntry {
    pub file: String,
    pub kind: TypeEntryKind,
    pub name: String,
    pub line: usize,
    pub cfg_gated: bool,
    pub doc: String,
    pub visibility: String,
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
pub enum TypeEntryKind {
    Struct,
    Enum,
    Union,
    Type,
}

/// What: one fn declaration at any depth (free, in-impl, in-trait,
/// extern-block); `brace_depth` records nesting at emission time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FnEntry {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub brace_depth: usize,
    pub doc: String,
    pub visibility: String,
    /// What: the INLINE module chain enclosing a module-level fn
    /// (`""` at file top level, `"m"` inside `mod m { .. }`,
    /// `"a::b"` nested). `None` when the fn is NOT module-level
    /// (nested in a body / impl / trait) or came from a macro
    /// template's token walk.
    ///
    /// Why: the hard declaration path = the file-derived module
    /// chain + this inline chain; the decl-driven API channel mints
    /// keys only for module-level fns and needs the chain for
    /// pub-reachability + canonical-binding selection.
    ///
    /// Where: set by `FileWalker::visit_item_fn` from its mod stack;
    /// consumed by the decl channel in characterize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// What: true when the declaration carries `#[doc(hidden)]`.
    ///
    /// Why: a hidden item is the publisher saying "not the public
    /// face" - it is disqualified from the decl-driven API channel.
    ///
    /// Where: set via `is_doc_hidden` in the walker's fn visitors.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub doc_hidden: bool,
}

/// What: one `mod X` declaration (with or without inline content).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModEntry {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub visibility: String,
    /// What: the INLINE module chain enclosing this declaration
    /// (`""` at file top; the declaring file's own chain comes from
    /// the file path).
    ///
    /// Why: pub-reachability walks module chains segment by
    /// segment; each segment's visibility lives on its ModEntry and
    /// the parent chain locates it.
    ///
    /// Where: set by `FileWalker::visit_item_mod`; consumed by the
    /// decl channel's reachability resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// What: true when the mod carries `#[doc(hidden)]` (hides the
    /// whole subtree from the public face).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub doc_hidden: bool,
}

/// What: one `use X::Y` statement; `reexport` flags `pub use ...`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UseEntry {
    pub file: String,
    pub reexport: bool,
    pub path: String,
    pub line: usize,
    /// What: the INLINE module chain enclosing the use statement
    /// (`""` at file top).
    ///
    /// Why: a `pub use` creates a soft binding AT its module chain;
    /// the binding's public path = file chain + this chain +
    /// binding name.
    ///
    /// Where: set by `FileWalker::visit_item_use`; consumed by the
    /// decl channel's binding builder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// What: true when the use carries `#[doc(hidden)]` (the
    /// binding is not part of the public face).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub doc_hidden: bool,
}

/// What: one macro call site (function-form or attribute-form). `kind`
/// distinguishes the two shapes; `args_count` / `arg_idents` /
/// `brace_depth` populate only for function-form invocations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MacroEntry {
    pub file: String,
    pub kind: MacroEntryKind,
    pub name: String,
    pub line: usize,
    pub expansion_unverified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_idents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brace_depth: Option<usize>,
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
pub enum MacroEntryKind {
    MacroInvocation,
    AttrMacro,
}

/// What: one `macro_rules! NAME { ... }` declaration site.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MacroDefEntry {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub visibility: String,
    pub macro_exported: bool,
}

/// What: one attribute occurrence (`#[...]` or `#![...]`). `inner`
/// distinguishes file-level inner attrs from item-level outer attrs;
/// `args` is the literal text inside the attribute's argument list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttrEntry {
    pub file: String,
    pub path: String,
    pub base: String,
    pub args: String,
    pub inner: bool,
    pub line: usize,
}

/// What: one trait name inside a `#[derive(...)]` list (one entry per
/// trait listed, not per derive attribute).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeriveEntry {
    pub file: String,
    #[serde(rename = "trait")]
    pub trait_name: String,
    pub line: usize,
}

/// What: one workspace-defined-pub type used at a factory-call /
/// match-arm / pattern site. `name` combines outer and inner segments
/// as `Outer::inner`. `kind_hint` identifies which architectural shape
/// the usage represents (currently only `FactoryCall`).
/// `expansion_unverified` is set when the usage was extracted from a
/// macro body's TokenStream rather than parsed syntax.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeUsageEntry {
    pub file: String,
    pub name: String,
    pub kind_hint: TypeUsageKind,
    pub line: usize,
    pub brace_depth: usize,
    pub expansion_unverified: bool,
    /// What: the path's ROOT segment when the usage was written with
    /// more segments than the recorded `Outer::inner` pair (e.g.
    /// `std::env::args()` records name `env::args`, qualifier `std`).
    /// `None` for bare two-segment paths.
    ///
    /// Why: item path resolution is the assumed mode of attribution
    /// (working/02). Truncating a fully-qualified path to its last two
    /// segments discarded the explicit root, so `std::`/external-
    /// qualified sites fell to the unresolved crate-local fallback and
    /// credited same-named workspace mods (R7 topic l's env::args
    /// class). The qualifier lets characterize resolve the root per
    /// language semantics.
    ///
    /// Where: set by `FileWalker::maybe_record_type_usage`; consumed
    /// by `compute_pattern_metrics`' per-site resolution gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
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
pub enum TypeUsageKind {
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
/// `ItemFacts::seams`.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum SeamKind {
    DynTraitObject,
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
    /// `know_rust_items.json` `seams` map.
    ///
    /// Why: serde_json maps require string keys; using a manual
    /// mapping keeps the wire schema explicit and bullet-proof against
    /// future field reorderings.
    ///
    /// Where: called at scan_workspace exit when collapsing per-file
    /// `HashMap<SeamKind, usize>` into the wire `BTreeMap<String, usize>`.
    pub fn wire_key(self) -> &'static str {
        match self {
            SeamKind::DynTraitObject => "dyn_trait_object",
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
