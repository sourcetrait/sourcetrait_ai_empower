/// What: one demanded-name record - the (rename-translated) source
/// name, its declaration kinds in the target (or `unknown`), and the
/// consumer-side streams that demanded it (use / reexport / ident /
/// fn_call / method_ref / type_usage).
///
/// Why: phase-1 conversion of the kr_consumer_trace.py prototype;
/// the wire shape mirrors the python report's record objects so the
/// parity check can diff outputs structurally.
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
/// Why: matches the python summary dict key-for-key (the zero-miss
/// bar reads `miss_count` + `pair_miss_count`); the misses ride
/// inside the summary exactly as the prototype emitted them.
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
/// Why: same top-level shape as the python's full dict ({summary,
/// hits, mod_namespace, pair_name_level}) so downstream readers and
/// the parity diff treat both generations identically.
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
