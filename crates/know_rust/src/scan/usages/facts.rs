#[allow(unused_imports)]
use crate::*;

/// What: aggregate facts produced by `know_rust scan usages` across
/// an entire workspace, serialized to `know_rust_usages.json` and
/// consumed by characterize.py as the AST cross-item usage feed.
///
/// Why: the retired regex-based rustscan implementation couldn't see
/// fn signatures, struct fields, type-alias RHS, or method
/// references; this AST-derived signal is namespaced under `ast_*`
/// keys so characterize.py merges cleanly without touching the
/// items-side wire shape.
///
/// Where: built in `scan::usages::walk::walk_workspace`; serialized
/// to `know_rust_usages.json` by `scan::usages::run::scan_usages`.
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct UsageFacts {
    pub tool_version: String,
    pub files_scanned: usize,
    pub files_parse_failed: usize,
    pub ast_fn_sig_usages: Vec<FnSigUsage>,
    pub ast_field_usages: Vec<FieldUsage>,
    pub ast_type_alias_usages: Vec<TypeAliasUsage>,
    pub ast_method_ref_usages: Vec<MethodRefUsage>,
}

/// What: per-file facts produced by `scan_file`. The aggregator
/// drains this into `UsageFacts` with the file path attached.
///
/// Why: keeping a per-file buffer lets the workspace walker stage
/// parse failures without leaving partial state in the aggregate
/// facts; the scanner has a single focused mutable target.
///
/// Where: owned by `scan_file` for the duration of one file's walk;
/// drained into `UsageFacts` by `walk_workspace` after the call
/// returns.
#[derive(Debug, Default)]
pub struct FileFacts {
    pub fn_sig_usages: Vec<FnSigUsage>,
    pub field_usages: Vec<FieldUsage>,
    pub type_alias_usages: Vec<TypeAliasUsage>,
    pub method_ref_usages: Vec<MethodRefUsage>,
}

/// What: a single type-identifier occurrence inside a function
/// signature. `fn_name` is the containing function; `position`
/// distinguishes parameter / return / generic-bound / where-clause
/// occurrences for downstream filtering.
///
/// Why: workspace-defined types used as function args / return types
/// signal real usage even when never instantiated via factory call.
/// This is the primary signal vs the retired regex-based scanner.
///
/// Where: emitted by `scan_file` walking each ItemFn (top-level + in
/// impls + in traits); read by characterize.py's pattern_metrics.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct FnSigUsage {
    pub file: String,
    pub fn_name: String,
    pub container: String,
    pub ident: String,
    pub position: FnPosition,
    pub line: usize,
    pub fn_visibility: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum FnPosition {
    Param,
    Return,
    GenericBound,
    WhereClause,
}

/// What: a single type-identifier occurrence inside a struct / enum
/// / union field declaration.
///
/// Why: types appearing as fields signal "this is a workspace-internal
/// composition primitive". Catches Frame-class types declared as
/// fields of larger structs.
///
/// Where: emitted by `scan_file` walking each ItemStruct / ItemEnum /
/// ItemUnion; read by characterize.py.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct FieldUsage {
    pub file: String,
    pub container: String,
    pub field_name: String,
    pub ident: String,
    pub position: FieldPosition,
    pub line: usize,
    pub container_visibility: String,
    pub field_visibility: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum FieldPosition {
    StructField,
    TupleStructField,
    EnumVariantField,
    EnumVariantTupleField,
    UnionField,
}

/// What: a single type-identifier occurrence inside a type alias RHS
/// (`pub type Foo = Bar<Baz>;` -> emits ident=Bar and ident=Baz).
///
/// Why: type aliases re-publish workspace-defined types under new
/// names; their RHS identifiers are real usage.
///
/// Where: emitted by `scan_file` walking each ItemType.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct TypeAliasUsage {
    pub file: String,
    pub alias_name: String,
    pub ident: String,
    pub line: usize,
    pub alias_visibility: String,
}

/// What: a single method-reference occurrence inside a fn body.
/// Captures multi-segment ExprPath `Type::method` patterns in
/// argument position of Call / MethodCall (not as receiver, not as
/// call head). `outer` is the path's last-but-one segment ("Type"),
/// `inner` is the final segment ("method"). Single-segment refs
/// (bare `method` ident) are NOT captured per 0.0.28 Phase 0
/// scoping - iced examples canonically use the multi-segment form.
///
/// Why: closes the iced Update gap. iced's
/// `iced::application(Clock::new, Clock::update, Clock::view)`
/// passes update as a fn pointer not a call; the existing scanner
/// only walked fn signatures + struct fields + type aliases and
/// couldn't see body expressions. characterize.py synthesizes
/// `method_ref:<outer>::<inner>` pattern_metrics entries from these
/// usages.
///
/// Where: emitted by `walk_fn_body` recursing through fn bodies
/// (top-level ItemFn, ImplItem::Fn, TraitItem::Fn with default
/// body).
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MethodRefUsage {
    pub file: String,
    pub container: String,
    pub outer: String,
    pub inner: String,
    pub line: usize,
}
