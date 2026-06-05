#[allow(unused_imports)]
use crate::*;

/// What: typed representation of the know_rust calibration knobs that
/// drive characterize.py-equivalent mode selection + use classification
/// and emit.py-equivalent picker scoring, family aggregation, cluster
/// surfacing, and filter sets.
///
/// Why: the python port reads calibration.toml at every invocation via
/// config.py + tomllib; the Rust port embeds the default toml via
/// `include_str!` and parses once at startup. End users override per
/// invocation via the global `-c <path>` CLI flag (`mem:developer`
/// pattern: parameterized over hardcoded). Sub-structs mirror the
/// toml's section tree so serde deserialization is mechanical.
///
/// Where: instantiated by `crate::config::loader::load_calibration` at
/// the top of `crate::run::run`; threaded into `characterize` and
/// `emit` subcommands when they land.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Calibration {
    pub mode: ModeConfig,
    pub classification: ClassificationConfig,
    pub picker: PickerConfig,
    pub filters: FiltersConfig,
}

/// What: thresholds driving characterize.py's `select_mode` decision
/// between single_dominant / co_equal_few / no_dominant histogram
/// modes, plus the seam-density floor that marks a workspace as
/// seam-dense (regional emit shape).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModeConfig {
    pub dominance_share: f64,
    pub coequal_topk: usize,
    pub coequal_share: f64,
    pub ambiguous_band: f64,
    pub seam_dense_per_kloc: f64,
}

/// What: threshold for characterize.py's `_classify_crate_use` flip
/// from end_with_dev_use to dev_with_end_use, based on the lib's
/// cross-crate pub usage count.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClassificationConfig {
    pub dev_with_end_threshold: usize,
}

/// What: emit.py picker calibration: top-N cap formula constants,
/// example weighting, method_ref family threshold, scoring boost
/// coefficients, and S1 cluster surfacing rule.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerConfig {
    pub top_n_floor: usize,
    pub sloc_divisor: usize,
    pub sloc_multiplier: f64,
    pub example: PickerExampleConfig,
    pub family: PickerFamilyConfig,
    pub score: PickerScoreConfig,
    pub cluster: PickerClusterConfig,
}

/// What: per-example weighting for the public-set boost. `weight_floor`
/// caps the minimum-example contribution; `saturation` and `threshold`
/// drive the example-count gamma boost; `test_weight` / `bench_weight`
/// scale non-curated example sites (curated examples/ stays at 1.0).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerExampleConfig {
    pub weight_floor: f64,
    pub saturation: usize,
    pub threshold: usize,
    pub test_weight: f64,
    pub bench_weight: f64,
}

/// What: minimum distinct workspace-defined outers required for a
/// `Type::method` method-reference family to enter pattern_metrics
/// as a `method_ref:_::<inner>` workspace-wide entry.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerFamilyConfig {
    pub method_ref_min_outers: usize,
}

/// What: combined-score boost coefficients applied in the picker's
/// score formula:
///   score = raw_count
///         * (1 + inter_boost * inter_ratio)
///         * (1 + pub_boost * is_pub)
///         * (1 + example_boost * min(example_count, saturation) / saturation)
///         * (example_threshold_boost if curated_example_count >= threshold else 1)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerScoreConfig {
    pub inter_boost: f64,
    pub pub_boost: f64,
    pub example_boost: f64,
    pub example_threshold_boost: f64,
}

/// What: S1 "Crate clusters (by name prefix)" sub-section parameters.
/// `threshold` is the workspace member count that triggers cluster
/// surfacing; `min_size` is the minimum member count for a single
/// cluster to render.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerClusterConfig {
    pub threshold: usize,
    pub min_size: usize,
}

/// What: filter sets applied at picker / synthesis time. Skip lists
/// for the method_ref family (outer + inner identifier blacklists),
/// plus the directory-segment skip set for the architectural
/// type-usage pool (those directories belong to the example-mining
/// scope, not the picker pool).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FiltersConfig {
    pub method_ref_outer_skip: Vec<String>,
    pub method_ref_inner_skip: Vec<String>,
    pub excluded_dir_segments: Vec<String>,
}

const DEFAULT_CALIBRATION_TOML: &str = include_str!("../../assets/calibration.toml");

impl Default for Calibration {
    fn default() -> Self {
        toml::from_str(DEFAULT_CALIBRATION_TOML)
            .expect("embedded calibration.toml fails to parse - build-time bug")
    }
}
