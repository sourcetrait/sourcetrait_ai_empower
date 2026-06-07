use crate::*;

/// What: per-crate metadata captured from a Cargo.toml: workspace-
/// relative directory, sorted dependency list, bin/lib presence flags,
/// and package keywords / categories / description.
///
/// Why: characterize.py's `find_crates` accumulated these fields per
/// the_user 2026-06-03 (`has_lib` / `[lib]` with significant `is_pub`
/// drives the 4-bucket use-classification). Preserving the shape here
/// keeps the downstream classifiers + fingerprint serializer in 1:1
/// correspondence with the python output.
///
/// Where: populated by `crate::characterize::cargo_toml::find_crates`;
/// consumed by `scan_crate`, `use_classification`, and `fingerprint`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CrateInfo {
    pub dir: String,
    pub deps: Vec<String>,
    pub has_bin: bool,
    pub has_lib: bool,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub description: String,
}

/// What: per-file items bucket assembled from `know_rust_items.json`,
/// indexed by workspace-relative file path so per-crate aggregation
/// resolves facts via the same path key the items walker emitted.
///
/// Why: mirrors `_build_items_index` in characterize.py, which buckets
/// the flat workspace-level json lists into per-file groups for
/// scan_crate to drain by path. Keeping the per-file shape close to
/// the wire schema keeps the aggregation loop straightforward.
///
/// Where: populated by `crate::characterize::items_index::build_items_index`
/// from the `ItemFacts` returned by `scan::items::workspace::scan_workspace`;
/// consumed by `crate::characterize::scan_crate::scan_crate`.
#[derive(Debug, Clone, Default)]
pub struct ItemFile {
    pub impls: Vec<ImplEntry>,
    pub traits: Vec<TraitEntry>,
    pub types: Vec<TypeEntry>,
    pub fns: Vec<FnEntry>,
    pub uses: Vec<UseEntry>,
    pub macros: Vec<MacroEntry>,
    pub derives: Vec<DeriveEntry>,
    pub macro_defs: Vec<MacroDefEntry>,
    pub mods: Vec<ModEntry>,
    pub type_usages: Vec<TypeUsageEntry>,
    pub example_type_usages: Vec<TypeUsageEntry>,
}

/// What: per-crate aggregate produced by `scan_crate` from the items
/// index. Carries the per-crate fact lists ready for assembly into
/// the workspace-level `all_facts`, plus the SLOC count for this
/// crate's source.
///
/// Why: matches characterize.py's scan_crate return dict; the
/// orchestrator drains each field into `all_facts` and the SLOC into
/// per_crate.sloc.
///
/// Where: returned by `crate::characterize::scan_crate::scan_crate`;
/// consumed by `crate::characterize::run::characterize`.
#[derive(Debug, Default)]
pub struct CrateAggregate {
    pub impls: Vec<serde_json::Value>,
    pub traits: Vec<serde_json::Value>,
    pub types: Vec<serde_json::Value>,
    pub fns: Vec<serde_json::Value>,
    pub uses: Vec<serde_json::Value>,
    pub macros: Vec<serde_json::Value>,
    pub derives: Vec<serde_json::Value>,
    pub macro_defs: Vec<serde_json::Value>,
    pub mods: Vec<serde_json::Value>,
    pub type_usages: Vec<serde_json::Value>,
    pub example_type_usages: Vec<serde_json::Value>,
    pub sloc: usize,
}

/// What: per-crate slot in the `fingerprint.json` `per_crate` map:
/// directory, SLOC, dependency list, per-kind fact counts, and an
/// empty `seams` placeholder matching the python output schema.
///
/// Why: characterize.py emits this exact shape; preserving the field
/// order + field names is load-bearing for byte-identical baseline
/// match.
///
/// Where: emitted by `crate::characterize::run::characterize` into
/// `Fingerprint::per_crate`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerCrateFingerprint {
    pub dir: String,
    pub sloc: usize,
    pub deps: Vec<String>,
    pub n_impls: usize,
    pub n_types: usize,
    pub n_traits: usize,
    pub n_fns: usize,
    pub seams: indexmap::IndexMap<String, usize>,
}

/// What: per-pattern metrics row in `pattern_metrics`: defining
/// crate (None when external), intra/inter counts + ratio, the
/// is_pub flag, and the example_count + curated_example_count
/// signals used by emit's picker scoring.
///
/// Why: characterize.py's `_compute_pattern_metrics` emits this
/// dict per pattern; preserving the field order keeps the wire JSON
/// stable.
///
/// Where: produced by `crate::characterize::pattern_metrics::compute`
/// for each pattern; serialized into the fingerprint's
/// `pattern_metrics` map.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PatternMetric {
    pub defining_crate: Option<String>,
    pub intra_count: usize,
    pub inter_count: usize,
    pub inter_ratio: f64,
    pub is_pub: bool,
    pub example_count: serde_json::Value,
    pub curated_example_count: usize,
    /// What: refactor phase R4 form sub-classification routing this
    /// pattern into a row of the prose-budget matrix (per
    /// `notes/know_rust/knowledge_product_authoring.md` cross-axis
    /// matrix). `None` for groups that have no sub-form discriminator
    /// (`ImplementationFunctions`, `TraitFunctions`, `Globals`).
    ///
    /// Why: the per-pick prose budget hint emitted into orientation.md
    /// at refactor phase R4 looks up `(PickGroup, SubForm, PickSet)`;
    /// persisting the sub_form here keeps the picker -> emit pipeline
    /// stateless about facts the classifier already inspected.
    ///
    /// Where: populated by classifier helpers in
    /// `crate::characterize::pattern_metrics::translate_to_group_keys`;
    /// consumed at emit time by
    /// `crate::config::calibration::ProseBudgetMatrix::budget_for`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_form: Option<SubForm>,
}

/// What: histogram entry in `pattern_histogram`: the `kind:name`
/// pattern string + its raw count. Top 40 entries are emitted in
/// rank order.
///
/// Why: python emits `[{"pattern": ..., "count": ...}, ...]`;
/// the struct fields preserve the order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PatternHistogramEntry {
    pub pattern: String,
    pub count: usize,
}

/// What: `selection` block in fingerprint.json describing how the
/// trace mode was chosen. `mode` is the final emit-routing mode;
/// `histogram_mode` is what the histogram alone would have picked.
///
/// Why: matches characterize.py's `select_mode` return shape.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Selection {
    pub mode: String,
    pub histogram_mode: String,
    pub structural_regional: bool,
    pub runner_up: Option<String>,
    pub top_share: f64,
    pub second_share: f64,
    pub topk_share: f64,
    pub notes: Vec<String>,
}

/// What: workspace-level 4-bucket use-classification: workspace
/// label (one of end_use/dev_use/end_with_dev_use/dev_with_end_use),
/// per-crate map, bucket counts, scaffolding crate list, and the
/// human-readable reasoning string.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceUseClassification {
    pub workspace: String,
    pub per_crate: indexmap::IndexMap<String, String>,
    pub buckets: indexmap::IndexMap<String, usize>,
    pub scaffolding_crates: Vec<String>,
    pub reasoning: String,
}

/// What: workspace_shape block in fingerprint.json: the shape label
/// (container / tight_framework / framework_with_users /
/// framework_product / submodule_aggregator / mixed / monolith /
/// empty), the structural signals dict, and the human-readable
/// reasoning string.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceShape {
    pub shape: String,
    pub signals: indexmap::IndexMap<String, serde_json::Value>,
    pub reasoning: String,
}

/// What: the `totals` block of fingerprint.json: workspace-wide
/// crate / sloc / impl / type / trait / fn counts plus the
/// example_rs_files count fed into emit's public-set log scaling.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Totals {
    pub crates: usize,
    pub sloc: usize,
    pub impls: usize,
    pub types: usize,
    pub traits: usize,
    pub fns: usize,
    pub example_rs_files: usize,
}

/// What: the `thresholds` block surfaced for auditability: the
/// declared mode-selection constants the picker used + a note
/// describing how to override (env vars; replaced by `-c` flag in
/// the rust port but the field stays for baseline parity).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Thresholds {
    #[serde(rename = "DOMINANCE_SHARE")]
    pub dominance_share: f64,
    #[serde(rename = "COEQUAL_TOPK")]
    pub coequal_topk: usize,
    #[serde(rename = "COEQUAL_SHARE")]
    pub coequal_share: f64,
    #[serde(rename = "AMBIGUOUS_BAND")]
    pub ambiguous_band: f64,
    #[serde(rename = "SEAM_DENSE_PER_KLOC")]
    pub seam_dense_per_kloc: f64,
    #[serde(rename = "_note")]
    pub note: String,
}

/// What: full fingerprint.json structure in the field order
/// characterize.py emits it. Carries the small auditable totals +
/// structural signals + the heavy pattern_metrics + per_crate maps
/// that downstream emit consumes.
///
/// Why: byte-identical baseline reproduction requires field ordering
/// match; struct order = serde output order.
///
/// Where: assembled in `crate::characterize::run::characterize`,
/// serialized to `fingerprint.json` via `serde_json::to_string_pretty`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Fingerprint {
    pub tool_version: String,
    pub repo_root: String,
    pub totals: Totals,
    pub workspace_roots: Vec<String>,
    pub components: Vec<Vec<String>>,
    pub n_components: usize,
    pub pattern_histogram: Vec<PatternHistogramEntry>,
    pub pattern_by_kind: indexmap::IndexMap<String, usize>,
    pub registration_macros: indexmap::IndexMap<String, usize>,
    pub seam_inventory: indexmap::IndexMap<String, usize>,
    pub seam_density_per_kloc: f64,
    pub selection: Selection,
    pub pattern_metrics: indexmap::IndexMap<String, PatternMetric>,
    pub workspace_use_classification: WorkspaceUseClassification,
    pub thresholds: Thresholds,
    pub per_crate: indexmap::IndexMap<String, PerCrateFingerprint>,
    pub workspace_shape: WorkspaceShape,
}

/// What: full facts.json structure - the workspace-level fact tables
/// + seams + AST-derived ast_type_refs + ast_method_refs. Each entry
/// is a `serde_json::Value` to preserve the python-side dict shape
/// without requiring a typed mirror for every fact variant.
///
/// Why: the wire schema is python's: items_walker (rust) emits the
/// per-fact shape; characterize.py adds a `crate` key per entry then
/// concatenates per-crate lists. The Value-typed entries preserve
/// the python's dict serialization exactly without needing a Rust
/// struct that mirrors every per-fact field.
///
/// Where: assembled in `crate::characterize::run::characterize`,
/// serialized to `facts.json`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceFacts {
    pub impls: Vec<serde_json::Value>,
    pub traits: Vec<serde_json::Value>,
    pub types: Vec<serde_json::Value>,
    pub fns: Vec<serde_json::Value>,
    pub uses: Vec<serde_json::Value>,
    pub macros: Vec<serde_json::Value>,
    pub derives: Vec<serde_json::Value>,
    pub macro_defs: Vec<serde_json::Value>,
    pub type_usages: Vec<serde_json::Value>,
    pub mods: Vec<serde_json::Value>,
    pub example_type_usages: Vec<serde_json::Value>,
    pub seams: indexmap::IndexMap<String, usize>,
    pub ast_type_refs: Vec<serde_json::Value>,
    pub ast_method_refs: Vec<serde_json::Value>,
    /// What: per-picked-pattern carry map propagated from
    /// `ItemFacts::carries`. Key is the `Pattern::Display` form
    /// (`<group_wire>:<name>`); value is the list of one-hop dependent
    /// names the reader needs to make sense of the picked item.
    ///
    /// Why: refactor phase R2 (per
    /// `notes/know_rust/tasks/picks-data-model-refactor.md`). Surfaces
    /// the carry signal at workspace level so downstream picker + emit
    /// phases (R3 + R5) can consume the typed transitive context.
    ///
    /// Where: populated in `crate::characterize::run::characterize`
    /// from `item_facts.carries` after the per-crate scan loop.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub carries: BTreeMap<Pattern, Vec<CarryEntry>>,
}
