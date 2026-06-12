/// What: one demanded-name record - the (rename-translated) source
/// name, its declaration kinds in the target (or `unknown`), and the
/// consumer-side streams that demanded it (use / reexport / ident /
/// fn_call / method_ref / type_usage).
///
/// Why: the wire shape is held stable across tool revisions so
/// report generations diff structurally.
///
/// Where: built by `demand_report` in `measure_demand::trace`;
/// serialized inside `DemandReport`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DemandRecord {
    pub name: String,
    pub kinds: Vec<String>,
    pub srcs: Vec<String>,
    /// What: how many consumer sites demanded this name (every
    /// stream insertion counts one site).
    ///
    /// Why: the consumer-weight blob scales by demand magnitude,
    /// not just presence; additive wire field.
    ///
    /// Where: tallied in `demand_report`; folded by
    /// `weights::fold_weights`.
    pub sites: usize,
}

/// What: the demand-trace summary block - name counts (with the full
/// miss records inline), the module-namespace bucket count, pair
/// totals by coverage tier, and glob imports.
///
/// Why: the zero-miss bar reads `miss_count` + `pair_miss_count`;
/// the full miss records ride inside the summary so the gate's
/// evidence is in one block.
///
/// Where: built by `demand_report`; printed as the stdout scoreboard
/// and serialized at `DemandReport::summary`.
#[derive(Debug, serde::Serialize)]
pub struct DemandSummary {
    pub demanded_names: usize,
    pub hits: usize,
    pub miss_count: usize,
    pub misses: Vec<DemandRecord>,
    pub mod_namespace_count: usize,
    /// What: demanded names the target serves by RE-EXPORTING a
    /// FOREIGN (non-workspace) crate's item or namespace - reported
    /// in their own bucket, not miss-counted (the mod_namespace
    /// pattern).
    ///
    /// Why: a `pub use futures::SinkExt` demand is real consumer
    /// demand on the target's surface, but no workspace pick can
    /// ever serve it; counting it a miss made the iced/libcosmic
    /// audits dishonest. The bucket keeps the gate meaningful until
    /// the foreign-API-surface emit section SERVES the class.
    ///
    /// Where: classified in `demand_report` from the target's
    /// foreign `pub use` facts; printed by `print_summary`.
    #[serde(default)]
    pub foreign_reexport_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign_reexports: Vec<DemandRecord>,
    pub pairs_total: usize,
    pub pair_exact: usize,
    pub pair_name_level: usize,
    pub pair_miss_count: usize,
    pub pair_misses: Vec<String>,
    /// What: demanded `<outer>::<inner>` pairs whose OUTER is a
    /// foreign re-exported namespace/root - the pair-tier mirror of
    /// the foreign_reexports bucket (non-gating).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pair_foreign: Vec<String>,
    pub globs: Vec<String>,
}

/// What: the full demand-trace report - summary plus the hit
/// records, the module-namespace import records, and the name-level
/// pair list.
///
/// Why: the stable top-level shape ({summary, hits, mod_namespace,
/// pair_name_level}) lets downstream readers and structural diffs
/// treat report generations identically.
///
/// Where: returned by `demand_report`; serialized to
/// `consumer_trace.json` by `measure_demand::run::measure_demand`.
#[derive(Debug, serde::Serialize)]
pub struct DemandReport {
    pub summary: DemandSummary,
    pub hits: Vec<DemandRecord>,
    pub mod_namespace: Vec<DemandRecord>,
    pub pair_name_level: Vec<String>,
    /// What: per demanded `<outer>::<inner>` pair, the consumer
    /// site count (additive wire field; the weight blob's pair
    /// magnitude).
    pub pair_sites: std::collections::BTreeMap<String, usize>,
}
