/// What: aggregate facts produced by the scanner across the entire
/// workspace. Indexed by relative file path; each file carries its
/// own per-item entries.
///
/// Why: characterize.py's existing rustscan.py extraction produces
/// per-file lists; this AST scanner emits the SUPPLEMENTAL signal
/// (fn signatures + struct fields + type aliases) the regex-based
/// scanner couldn't reach. Output schema is namespaced under
/// `ast_*` keys so characterize.py can merge cleanly without
/// touching rustscan.py outputs.
///
/// Where: built in walk_workspace(); serialized to scan.json in run().
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct Facts {
    pub(crate) tool_version: String,
    pub(crate) files_scanned: usize,
    pub(crate) files_parse_failed: usize,
    pub(crate) ast_fn_sig_usages: Vec<FnSigUsage>,
    pub(crate) ast_field_usages: Vec<FieldUsage>,
    pub(crate) ast_type_alias_usages: Vec<TypeAliasUsage>,
    pub(crate) ast_method_ref_usages: Vec<MethodRefUsage>,
}

/// What: per-file facts produced by scan_file(). The aggregator
/// reads this back into Facts with the file path attached.
#[derive(Debug, Default)]
pub(crate) struct FileFacts {
    pub(crate) fn_sig_usages: Vec<FnSigUsage>,
    pub(crate) field_usages: Vec<FieldUsage>,
    pub(crate) type_alias_usages: Vec<TypeAliasUsage>,
    pub(crate) method_ref_usages: Vec<MethodRefUsage>,
}

/// What: a single type-identifier occurrence inside a function
/// signature. `fn_name` is the containing function; `position`
/// distinguishes parameter / return / generic-bound / where-clause
/// occurrences for downstream filtering.
///
/// Why: workspace-defined types used as function args / return types
/// signal real usage even when never instantiated via factory call.
/// This is the primary missing signal vs the regex-based rustscan.py.
///
/// Where: emitted by scan_file() walking each ItemFn (top-level + in
/// impls + in traits); read by characterize.py's pattern_metrics.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct FnSigUsage {
    pub(crate) file: String,
    pub(crate) fn_name: String,
    pub(crate) container: String,
    pub(crate) ident: String,
    pub(crate) position: FnPosition,
    pub(crate) line: usize,
    pub(crate) fn_visibility: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FnPosition {
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
/// Where: emitted by scan_file() walking each ItemStruct / ItemEnum /
/// ItemUnion; read by characterize.py.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct FieldUsage {
    pub(crate) file: String,
    pub(crate) container: String,
    pub(crate) field_name: String,
    pub(crate) ident: String,
    pub(crate) position: FieldPosition,
    pub(crate) line: usize,
    pub(crate) container_visibility: String,
    pub(crate) field_visibility: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FieldPosition {
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
/// Where: emitted by scan_file() walking each ItemType.
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct TypeAliasUsage {
    pub(crate) file: String,
    pub(crate) alias_name: String,
    pub(crate) ident: String,
    pub(crate) line: usize,
    pub(crate) alias_visibility: String,
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
/// Where: emitted by walk_fn_body() recursing through fn bodies
/// (top-level ItemFn, ImplItem::Fn, TraitItem::Fn with default body).
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct MethodRefUsage {
    pub(crate) file: String,
    pub(crate) container: String,
    pub(crate) outer: String,
    pub(crate) inner: String,
    pub(crate) line: usize,
}
