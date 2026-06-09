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
    /// What: function CALL-HEAD usages (bare or qualified call paths
    /// with a lowercase-initial callee). Fills the utilities FreeFn
    /// channel: standalone fns are a picks-data group whose capture
    /// was missing (the consumer-trace test surfaced nushell's
    /// embedding API - eval_block / parse / create_default_context -
    /// as invisible to every pick channel).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ast_fn_call_usages: Vec<FnCallUsage>,
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
    pub fn_call_usages: Vec<FnCallUsage>,
}

/// What: one function call-head occurrence. `name` is the callee's
/// final path segment (lowercase-initial only; uppercase heads are
/// tuple-struct / variant constructors covered by the items walker's
/// type_usage channel). `qualifier` is the path ROOT for qualified
/// calls (`nu_parser::parse(..)` -> name `parse`, qualifier
/// `nu_parser`); `None` for bare imported / local calls.
///
/// Why: standalone fns are the utilities group's FreeFn members in
/// the picks-data model, but no capture channel fed them - a
/// consumer demanding `eval_block` or `create_default_context` had
/// no pick to land on. Bare calls resolve through the file's
/// imports at characterize time; the per-site resolution gate keeps
/// std / external callees out, per the workspace-origin rule.
///
/// Where: emitted by `walk_call` / `walk_method_call` in
/// `scan::usages::scan`; consumed by `compute_pattern_metrics`' free
/// fn synthesis.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct FnCallUsage {
    pub file: String,
    pub name: String,
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
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
    /// What: the type path's lowercase-initial ROOT segment when the
    /// ident was written module-qualified (`std::io::Error` -> ident
    /// `Error`, qualifier `std`; `git2::Status` -> qualifier `git2`).
    /// `None` for unqualified / type-led paths.
    ///
    /// Why: path resolution is the assumed attribution mode
    /// (working/02); without the root, qualified externals fell to the
    /// unresolved crate-local fallback and credited same-named
    /// workspace types (structure:Error class).
    ///
    /// Where: set by the `collect_idents` family in
    /// `scan::usages::scan`; consumed by `compute_pattern_metrics`'
    /// pub_type synthesis gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum FnPosition {
    Param,
    Return,
    GenericBound,
    WhereClause,
    /// Type argument written in a call's turbofish
    /// (`eval_block::<WithoutDebug>(..)`). Real type usage that the
    /// signature walk cannot see.
    CallTurbofish,
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
    /// What: module-qualified root segment for the field type's path,
    /// same semantics as `FnSigUsage::qualifier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
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
    /// What: module-qualified root segment for the RHS type's path,
    /// same semantics as `FnSigUsage::qualifier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
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
    /// What: the path's ROOT segment when the reference carried more
    /// segments than the recorded `outer::inner` pair
    /// (`git2::Status::INDEX_NEW` -> outer `Status`, qualifier
    /// `git2`). `None` for bare two-segment refs.
    ///
    /// Why: same truncation gap as `TypeUsageEntry::qualifier` - the
    /// explicit root is language semantics the resolution gate must
    /// see.
    ///
    /// Where: set by `emit_method_ref_if_path`; consumed by
    /// `compute_pattern_metrics`' method_ref / assoc-const synthesis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
}
