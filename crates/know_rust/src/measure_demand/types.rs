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
    pub pairs_total: usize,
    pub pair_exact: usize,
    pub pair_name_level: usize,
    pub pair_miss_count: usize,
    pub pair_misses: Vec<String>,
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
}
