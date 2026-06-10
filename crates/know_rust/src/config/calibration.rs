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
/// coefficients, S1 cluster surfacing rule, the per-pick prose-budget
/// matrix, and the form sub-classifier thresholds (general-signal
/// only; no per-subject allowlists).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PickerConfig {
    pub top_n_floor: usize,
    pub sloc_divisor: usize,
    pub sloc_multiplier: f64,
    pub example: PickerExampleConfig,
    pub family: PickerFamilyConfig,
    pub score: PickerScoreConfig,
    pub cluster: PickerClusterConfig,
    pub classifier: ClassifierConfig,
    pub prose_budget: ProseBudgetMatrix,
    pub cap_matrix: CapMatrix,
    pub consumer_weight: ConsumerWeightConfig,
}

/// What: knobs for the consumer-demand weight term. `site_weight` is
/// the per-demand-site BASE score a zero-usage decl-channel pair key
/// earns (entering the PUBLIC set as public-by-consumption);
/// `usage_boost` is the additive per-site boost for keys already
/// carrying usage-derived public scores.
///
/// Why: declared-but-internally-unused API has no usage signal by
/// construction; revealed consumer demand (the weight blob) is its
/// mechanical significance source, and the knobs keep the term
/// calibratable without recompiling.
///
/// Where: held inside `PickerConfig`; consumed by
/// `emit::picker::compute_significance_sets` when a weight blob is
/// supplied.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConsumerWeightConfig {
    pub site_weight: f64,
    pub usage_boost: f64,
}

/// What: form sub-classifier thresholds the structure + traits
/// classifiers consult. Each rests on a GENERAL structural signal
/// (count / ratio derivable from any workspace's AST) - no per-subject
/// hardcoded name-lists (mechanical-broad, subagent-fine per
/// `notes/know_rust/working/05_calibration.md`):
///
/// - `foundational_min_total` -> structure foundational vs incidental
///   split (default 30; intra+inter+example >= threshold means
///   foundational).
/// - `lifecycle_impl_threshold` -> traits lifecycle vs marker split
///   on workspace impl count (default 5).
///
/// The `Configuring` group (configured-via-attributes derives +
/// configuring attribute-macros) carries NO mechanical sub-form: the
/// removed `configured_derives` allowlist was a per-subject cheat that
/// did not generalize to unseen subjects. The broad group mark is the
/// whole mechanical signal; the subagent thought-experiment does the
/// fine subclassification.
///
/// Why: keeps the only knobs that ride a general signal in
/// calibration.toml (lower the lifecycle threshold for a small
/// ecosystem without recompiling) and keeps cheats out of the source.
///
/// Where: held inside `PickerConfig`; consumed by classifier helpers
/// in `crate::characterize::pattern_metrics`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClassifierConfig {
    pub foundational_min_total: usize,
    pub lifecycle_impl_threshold: usize,
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
/// the directory-segment skip set for the architectural type-usage
/// pool (those directories belong to the example-mining scope, not
/// the picker pool), and the pattern-key substring skip set that
/// suppresses noise families like `Value::test_*` from the
/// pattern_metrics layer (NF5).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FiltersConfig {
    pub method_ref_outer_skip: Vec<String>,
    pub method_ref_inner_skip: Vec<String>,
    pub excluded_dir_segments: Vec<String>,
    /// What: substring blacklist applied against each pattern key
    /// (`kind:name` pre-translation) at the start of
    /// `compute_pattern_metrics`. Any pattern whose key contains
    /// any listed substring is dropped before metric computation +
    /// downstream `translate_to_group_keys` + classifier runs.
    ///
    /// Why: NF5 - nushell's `Value::test_*` family + `Span::test_data`
    /// dominate the type_usages divergence (~3000+ entries per the
    /// 0.0.34 phase 4 pass B audit), contaminating the
    /// `implementation_functions:*::test_*` family + the
    /// aggregated `structure:Value` carry. The default `::test_`
    /// substring catches the canonical test-helper shape while
    /// leaving production methods uninhibited. Externalized via
    /// calibration.toml so per-workspace tuning (e.g. adding
    /// `::_test_` or `::__test_` shapes) is a config edit, not a
    /// recompile.
    ///
    /// Where: consumed by
    /// `crate::characterize::pattern_metrics::compute_pattern_metrics`
    /// at the seen_patterns iteration head.
    #[serde(default = "default_pattern_skip_substrings")]
    pub pattern_skip_substrings: Vec<String>,
}

fn default_pattern_skip_substrings() -> Vec<String> {
    vec!["::test_".to_string()]
}

/// What: per-set prose budget hints (character counts) for one row
/// of the matrix. Six cells, one per `PickSet` variant.
///
/// Why: the prose-budget matrix is keyed by `(group, sub_form,
/// set)`; the cell row is selected by `(group, sub_form)` and the
/// `for_set` lookup returns the set's column. Tuned 2026-06-05 from
/// the bevy spot test (`notes/know_rust/knowledge_product_authoring.md`).
///
/// Where: held inside `ProseBudgetMatrix`'s 11 row fields; surfaced
/// per pick via `ProseBudgetMatrix::budget_for`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProseBudgetCell {
    pub architecture: usize,
    pub public: usize,
    pub inter_crate: usize,
    pub clique: usize,
    pub intra_crate: usize,
    pub inner_crate: usize,
}

impl ProseBudgetCell {
    /// What: return the char budget for the given pick set.
    ///
    /// Why: callers know the set from the picker's pre-aggregation;
    /// the cell encodes all six set columns inline.
    ///
    /// Where: called by `ProseBudgetMatrix::budget_for` after the
    /// row is selected.
    pub fn for_set(&self, set: PickSet) -> usize {
        match set {
            PickSet::Architecture => self.architecture,
            PickSet::Public => self.public,
            PickSet::InterCrate => self.inter_crate,
            PickSet::Clique => self.clique,
            PickSet::IntraCrate => self.intra_crate,
            PickSet::InnerCrate => self.inner_crate,
        }
    }
}

/// What: the 11-row prose-budget matrix per the R4 refactor's
/// (group, sub_form, set) cell encoding. Rows correspond to the
/// cross-axis matrix in
/// `notes/know_rust/knowledge_product_authoring.md`.
///
/// Why: the kp pipeline Stage C drafting subagent receives a
/// per-pick prose budget hint from this matrix; the budget tells
/// the drafter how many non-inferrable chars the pick legitimately
/// warrants. Foundational structures + lifecycle traits + configured
/// derives earn rich budgets; marker derives + globals get thin
/// budgets.
///
/// Where: loaded from `calibration.toml` [picker.prose_budget.*]
/// sections via serde; consumed at emit time via `budget_for` to
/// populate each per-pick `EnrichedEntry::budget_hint`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProseBudgetMatrix {
    pub structure_foundational: ProseBudgetCell,
    pub structure_incidental: ProseBudgetCell,
    pub traits_lifecycle: ProseBudgetCell,
    pub traits_marker: ProseBudgetCell,
    pub configuring: ProseBudgetCell,
    pub implementation_functions: ProseBudgetCell,
    pub trait_functions: ProseBudgetCell,
    pub utilities_free_fn: ProseBudgetCell,
    pub utilities_macro: ProseBudgetCell,
    pub globals: ProseBudgetCell,
}

impl ProseBudgetMatrix {
    /// What: look up the per-pick prose budget for `(group, sub_form,
    /// set)`. Returns the char hint Stage C should target for the
    /// pick's prose. Falls back to the thinner row when a group has
    /// a sub-form discriminator but the classifier returned None
    /// (defensive default).
    ///
    /// Why: the matrix's row selection encodes the form sub-
    /// classification; callers should only have to pass the three
    /// raw axes and get the budget back without re-deriving the row.
    ///
    /// Where: called by `crate::emit::instance::candidate_instances`
    /// when constructing each `EnrichedEntry`; the budget hint
    /// flows into orientation.md's per-pick header.
    pub fn budget_for(
        &self,
        group: PickGroup,
        sub_form: Option<SubForm>,
        set: PickSet,
    ) -> usize {
        let cell: &ProseBudgetCell = match (group, sub_form) {
            (PickGroup::Structure, Some(SubForm::Foundational)) => &self.structure_foundational,
            (PickGroup::Structure, Some(SubForm::Incidental)) => &self.structure_incidental,
            (PickGroup::Structure, _) => &self.structure_incidental,
            (PickGroup::Traits, Some(SubForm::Lifecycle)) => &self.traits_lifecycle,
            (PickGroup::Traits, Some(SubForm::Marker)) => &self.traits_marker,
            (PickGroup::Traits, _) => &self.traits_marker,
            // Configuring takes one broad budget hint (no mechanical
            // sub-form); the subagent thought-experiment sets actual depth.
            (PickGroup::Configuring, _) => &self.configuring,
            (PickGroup::ImplementationFunctions, _) => &self.implementation_functions,
            (PickGroup::TraitFunctions, _) => &self.trait_functions,
            (PickGroup::Utilities, Some(SubForm::FreeFn)) => &self.utilities_free_fn,
            (PickGroup::Utilities, Some(SubForm::Macro)) => &self.utilities_macro,
            (PickGroup::Utilities, _) => &self.utilities_macro,
            (PickGroup::Globals, _) => &self.globals,
        };
        cell.for_set(set)
    }
}

/// What: per-set cap multipliers for the R4b cap matrix. Multiplies
/// the base SLOC-scaled cap to size the picked pool per significance
/// set (architecture / public / inter_crate / clique / intra_crate /
/// inner_crate).
///
/// Why: replaces the single global top-N cap with per-cell tuning.
/// Cell weights live in calibration.toml
/// `[picker.cap_matrix.set]`; the_user adjusts in calibration.toml
/// without recompile.
///
/// Where: held inside `CapMatrix::set`; consumed by `CapMatrix::cap_for`
/// when applying the matrix to a (group, set, base_cap) triple.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapMatrixSetMultipliers {
    pub architecture: f64,
    pub public: f64,
    pub inter_crate: f64,
    pub clique: f64,
    pub intra_crate: f64,
    pub inner_crate: f64,
}

impl CapMatrixSetMultipliers {
    /// What: return the multiplier for the given pick set.
    ///
    /// Where: called by `CapMatrix::cap_for`.
    pub fn for_set(&self, set: PickSet) -> f64 {
        match set {
            PickSet::Architecture => self.architecture,
            PickSet::Public => self.public,
            PickSet::InterCrate => self.inter_crate,
            PickSet::Clique => self.clique,
            PickSet::IntraCrate => self.intra_crate,
            PickSet::InnerCrate => self.inner_crate,
        }
    }
}

/// What: per-group cap multipliers for the R4b cap matrix. Multiplies
/// the base SLOC-scaled cap to size the picked pool per pick group
/// (traits / trait_functions / structure / implementation_functions /
/// derives / utilities / globals).
///
/// Why: replaces the group-blind single cap with per-group cell
/// weights. Cell weights live in calibration.toml
/// `[picker.cap_matrix.group]`; the_user adjusts without recompile.
///
/// Where: held inside `CapMatrix::group`; consumed by
/// `CapMatrix::cap_for`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapMatrixGroupMultipliers {
    pub traits: f64,
    pub trait_functions: f64,
    pub structure: f64,
    pub implementation_functions: f64,
    pub configuring: f64,
    pub utilities: f64,
    pub globals: f64,
}

impl CapMatrixGroupMultipliers {
    /// What: return the multiplier for the given pick group.
    ///
    /// Where: called by `CapMatrix::cap_for`.
    pub fn for_group(&self, group: PickGroup) -> f64 {
        match group {
            PickGroup::Traits => self.traits,
            PickGroup::TraitFunctions => self.trait_functions,
            PickGroup::Structure => self.structure,
            PickGroup::ImplementationFunctions => self.implementation_functions,
            PickGroup::Configuring => self.configuring,
            PickGroup::Utilities => self.utilities,
            PickGroup::Globals => self.globals,
        }
    }
}

/// What: R4b top-N cap matrix - per-(PickGroup, PickSet) cap derived
/// from a base SLOC-scaled cap via multiplicative inheritance:
/// `cap[g][s] = max(floor, round(base * group_mult[g] * set_mult[s]))`.
///
/// Why: replaces the single global top-N cap formula with cell-
/// specific tuning. Lets the picker grow public / inter_crate /
/// clique / inner_crate sets to push big-repo kp output toward the
/// 200-300K token target per `mem:know-rust-kp-output-token-target`
/// without lifting the architecture set or smaller workspaces above
/// their natural cap. Multiplicative inheritance (13 stored
/// multipliers: 7 group + 6 set) preferred over independent grid
/// axes (42 stored cell values) per the_user 2026-06-05 direction in
/// `notes/know_rust/working/05_calibration.md`.
///
/// Where: loaded from calibration.toml `[picker.cap_matrix.*]` via
/// serde; consumed by per-(group, set) cap computation in
/// `crate::emit::picker::compute_significance_sets` and
/// `per_crate_picks` (R4b phase 2).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapMatrix {
    pub set: CapMatrixSetMultipliers,
    pub group: CapMatrixGroupMultipliers,
}

impl CapMatrix {
    /// What: compute the cap for the given (group, set, base_cap)
    /// triple. `base_cap` is the existing SLOC-scaled formula's
    /// output for the appropriate scope (workspace SLOC for
    /// workspace-wide sets; per-crate SLOC for per-crate sets).
    /// `floor` is the calibration's minimum cap.
    ///
    /// Math:
    ///   scaled = base_cap * group_mult[group] * set_mult[set]
    ///   cap    = max(floor, round(scaled))
    ///
    /// Why: callers compute the base cap once per scope then apply
    /// the matrix per (group, set) bucket. The floor enforces a
    /// minimum sample size even when multipliers shrink the cap
    /// below the global floor.
    ///
    /// Where: called by `crate::emit::picker` per (group, set) when
    /// applying the cap to a scored pattern list (R4b phase 2).
    pub fn cap_for(
        &self,
        group: PickGroup,
        set: PickSet,
        base_cap: usize,
        floor: usize,
    ) -> usize {
        let scaled = (base_cap as f64)
            * self.group.for_group(group)
            * self.set.for_set(set);
        let rounded = scaled.round() as usize;
        rounded.max(floor)
    }
}

const DEFAULT_CALIBRATION_TOML: &str = include_str!("../../assets/calibration.toml");

impl Default for Calibration {
    fn default() -> Self {
        toml::from_str(DEFAULT_CALIBRATION_TOML)
            .expect("embedded calibration.toml fails to parse - build-time bug")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_budget_default_matches_tuned_matrix() {
        // The shipped assets/calibration.toml encodes the matrix from
        // notes/know_rust/knowledge_product_authoring.md. This test
        // pins the architecture-column values per row so any
        // unintended drift in the toml fails fast.
        let cal = Calibration::default();
        let pb = &cal.picker.prose_budget;
        let arch = PickSet::Architecture;
        assert_eq!(
            pb.budget_for(PickGroup::Structure, Some(SubForm::Foundational), arch),
            800
        );
        assert_eq!(
            pb.budget_for(PickGroup::Structure, Some(SubForm::Incidental), arch),
            400
        );
        assert_eq!(
            pb.budget_for(PickGroup::Traits, Some(SubForm::Lifecycle), arch),
            1200
        );
        assert_eq!(
            pb.budget_for(PickGroup::Traits, Some(SubForm::Marker), arch),
            500
        );
        assert_eq!(
            pb.budget_for(PickGroup::Configuring, None, arch),
            1200
        );
        assert_eq!(
            pb.budget_for(PickGroup::ImplementationFunctions, None, arch),
            300
        );
        assert_eq!(pb.budget_for(PickGroup::TraitFunctions, None, arch), 300);
        assert_eq!(
            pb.budget_for(PickGroup::Utilities, Some(SubForm::FreeFn), arch),
            250
        );
        assert_eq!(
            pb.budget_for(PickGroup::Utilities, Some(SubForm::Macro), arch),
            350
        );
        assert_eq!(pb.budget_for(PickGroup::Globals, None, arch), 100);
    }

    #[test]
    fn prose_budget_set_columns_descend_to_intra() {
        // For each row that has a sub-form discriminator, verify the
        // set-column ordering Architecture >= Public >= InterCrate >=
        // Clique and Architecture >= InnerCrate >= IntraCrate. Loose
        // monotonicity: some adjacent cells may be equal.
        let cal = Calibration::default();
        let pb = &cal.picker.prose_budget;
        for (g, sf) in [
            (PickGroup::Structure, Some(SubForm::Foundational)),
            (PickGroup::Structure, Some(SubForm::Incidental)),
            (PickGroup::Traits, Some(SubForm::Lifecycle)),
            (PickGroup::Traits, Some(SubForm::Marker)),
            (PickGroup::Configuring, None),
            (PickGroup::ImplementationFunctions, None),
            (PickGroup::TraitFunctions, None),
            (PickGroup::Utilities, Some(SubForm::FreeFn)),
            (PickGroup::Utilities, Some(SubForm::Macro)),
            (PickGroup::Globals, None),
        ] {
            let arch = pb.budget_for(g, sf, PickSet::Architecture);
            let public = pb.budget_for(g, sf, PickSet::Public);
            let inter = pb.budget_for(g, sf, PickSet::InterCrate);
            let clique = pb.budget_for(g, sf, PickSet::Clique);
            let intra = pb.budget_for(g, sf, PickSet::IntraCrate);
            let inner = pb.budget_for(g, sf, PickSet::InnerCrate);
            assert!(
                arch >= public,
                "{:?} {:?}: arch {} < public {}",
                g, sf, arch, public
            );
            assert!(
                public >= inter,
                "{:?} {:?}: public {} < inter {}",
                g, sf, public, inter
            );
            assert!(
                inter >= clique,
                "{:?} {:?}: inter {} < clique {}",
                g, sf, inter, clique
            );
            assert!(
                arch >= inner,
                "{:?} {:?}: arch {} < inner {}",
                g, sf, arch, inner
            );
            assert!(
                inner >= intra,
                "{:?} {:?}: inner {} < intra {}",
                g, sf, inner, intra
            );
        }
    }

    #[test]
    fn prose_budget_none_falls_back_to_thin_row() {
        // When a group has a sub-form discriminator but the classifier
        // returned None (defensive path), the matrix lookup should
        // resolve to the THINNER row to avoid overspending budget on
        // an unclassified pick.
        let cal = Calibration::default();
        let pb = &cal.picker.prose_budget;
        let set = PickSet::Architecture;
        // structure None -> incidental
        assert_eq!(
            pb.budget_for(PickGroup::Structure, None, set),
            pb.budget_for(PickGroup::Structure, Some(SubForm::Incidental), set),
        );
        // traits None -> marker
        assert_eq!(
            pb.budget_for(PickGroup::Traits, None, set),
            pb.budget_for(PickGroup::Traits, Some(SubForm::Marker), set),
        );
        // configuring has a single row; None is the canonical path.
        assert_eq!(pb.budget_for(PickGroup::Configuring, None, set), 1200);
        // utilities None -> macro (the picker's only utilities source)
        assert_eq!(
            pb.budget_for(PickGroup::Utilities, None, set),
            pb.budget_for(PickGroup::Utilities, Some(SubForm::Macro), set),
        );
    }

    #[test]
    fn prose_budget_configuring_richest_at_architecture() {
        // Sanity: configuring + lifecycle traits should be the richest
        // matrix rows at the architecture column. The kp pipeline's
        // foundational / lifecycle / configuring cells are what the
        // matrix earmarks for the largest budget; globals the thinnest.
        let cal = Calibration::default();
        let pb = &cal.picker.prose_budget;
        let arch = PickSet::Architecture;
        let configuring = pb.budget_for(PickGroup::Configuring, None, arch);
        let lifecycle = pb.budget_for(PickGroup::Traits, Some(SubForm::Lifecycle), arch);
        let foundational = pb.budget_for(PickGroup::Structure, Some(SubForm::Foundational), arch);
        let globals = pb.budget_for(PickGroup::Globals, None, arch);
        assert!(configuring >= foundational);
        assert!(lifecycle >= foundational);
        assert!(configuring > globals);
        assert!(foundational > globals);
    }

    #[test]
    fn cap_matrix_default_loads_with_tuned_weights() {
        // Pins the embedded calibration.toml's R4b cap matrix to the
        // 2026-06-07 the_user-tuned weights so unintended drift in
        // the toml fails fast. Set widening hierarchy:
        //   architecture < public = inter_crate = clique < intra_crate < inner_crate
        // (intersection-limited sets at smaller weights; per-crate
        // sets lifted higher; inner_crate widest as per-crate own
        // architecture is the bulkiest signal pool.)
        // Group multipliers: traits + structure (architectural
        // backbone) at 1.5; configuring at 1.2; functions sample at
        // 1.0; utilities + globals lifted to 1.5 / 1.2 (the original
        // 0.8 / 0.5 trim suppressed too much of bevy's contribution).
        let cal = Calibration::default();
        let cm = &cal.picker.cap_matrix;
        assert_eq!(cm.set.for_set(PickSet::Architecture), 2.5);
        assert_eq!(cm.set.for_set(PickSet::Public), 3.5);
        assert_eq!(cm.set.for_set(PickSet::InterCrate), 3.5);
        assert_eq!(cm.set.for_set(PickSet::Clique), 3.5);
        assert_eq!(cm.set.for_set(PickSet::IntraCrate), 4.0);
        assert_eq!(cm.set.for_set(PickSet::InnerCrate), 5.0);
        assert_eq!(cm.group.for_group(PickGroup::Traits), 1.5);
        assert_eq!(cm.group.for_group(PickGroup::Structure), 1.5);
        assert_eq!(cm.group.for_group(PickGroup::Configuring), 1.2);
        assert_eq!(cm.group.for_group(PickGroup::ImplementationFunctions), 1.0);
        assert_eq!(cm.group.for_group(PickGroup::TraitFunctions), 1.0);
        assert_eq!(cm.group.for_group(PickGroup::Utilities), 1.5);
        assert_eq!(cm.group.for_group(PickGroup::Globals), 1.2);
    }

    #[test]
    fn cap_matrix_cap_for_multiplicative_math() {
        // Verifies the matrix arithmetic:
        //   cap = max(floor, round(base * group_mult * set_mult))
        let cal = Calibration::default();
        let cm = &cal.picker.cap_matrix;
        let floor = cal.picker.top_n_floor;

        // impl_fns (1.0) * Architecture (2.5): base 20 * 2.5 = 50.
        assert_eq!(
            cm.cap_for(PickGroup::ImplementationFunctions, PickSet::Architecture, 20, floor),
            50,
            "1.0 group * 2.5 set"
        );

        // Traits (1.5) * Architecture (2.5): 20 * 1.5 * 2.5 = 75.
        assert_eq!(
            cm.cap_for(PickGroup::Traits, PickSet::Architecture, 20, floor),
            75,
            "1.5 group * 2.5 set"
        );

        // Structure (1.5) * InterCrate (3.5): 20 * 1.5 * 3.5 = 105.
        assert_eq!(
            cm.cap_for(PickGroup::Structure, PickSet::InterCrate, 20, floor),
            105,
            "1.5 * 3.5 stacked"
        );

        // Configuring (1.2) * Clique (3.5): 20 * 1.2 * 3.5 = 84.
        assert_eq!(
            cm.cap_for(PickGroup::Configuring, PickSet::Clique, 20, floor),
            84,
            "1.2 * 3.5"
        );

        // Inner_crate widest (5.0). Globals (1.2) * Inner (5.0):
        // 20 * 1.2 * 5.0 = 120.
        assert_eq!(
            cm.cap_for(PickGroup::Globals, PickSet::InnerCrate, 20, floor),
            120,
            "1.2 * 5.0 inner widest"
        );

        // Floor enforcement: base 2 * impl_fns 1.0 * Architecture 2.5
        // = 5; floor (7) wins.
        assert_eq!(
            cm.cap_for(PickGroup::ImplementationFunctions, PickSet::Architecture, 2, floor),
            7,
            "floor enforced when scaled below floor"
        );

        // Zero base hits floor regardless of multipliers.
        assert_eq!(
            cm.cap_for(PickGroup::Traits, PickSet::Architecture, 0, floor),
            7,
            "zero base hits floor"
        );
    }

    #[test]
    fn cap_matrix_set_widening_hierarchy() {
        // Design intent: set widening hierarchy is
        //   architecture < public = inter_crate = clique < intra_crate < inner_crate
        // Architecture is the most intersection-limited (public AND
        // inter), so its weight stays smallest. inner_crate is the
        // per-crate own-architecture set with the most patterns
        // available, gets the widest cap. intra_crate (per-crate
        // consumption) sits between workspace-wide non-arch sets
        // and inner.
        let cal = Calibration::default();
        let cm = &cal.picker.cap_matrix;
        let floor = cal.picker.top_n_floor;
        let base = 20;

        for group in [
            PickGroup::Traits,
            PickGroup::Structure,
            PickGroup::Configuring,
            PickGroup::ImplementationFunctions,
        ] {
            let arch = cm.cap_for(group, PickSet::Architecture, base, floor);
            let public = cm.cap_for(group, PickSet::Public, base, floor);
            let inter = cm.cap_for(group, PickSet::InterCrate, base, floor);
            let clique = cm.cap_for(group, PickSet::Clique, base, floor);
            let intra = cm.cap_for(group, PickSet::IntraCrate, base, floor);
            let inner = cm.cap_for(group, PickSet::InnerCrate, base, floor);

            // arch is the smallest cap (intersection-limited).
            assert!(public >= arch, "{:?}: public {} < arch {}", group, public, arch);
            assert!(inter >= arch, "{:?}: inter {} < arch {}", group, inter, arch);
            assert!(clique >= arch, "{:?}: clique {} < arch {}", group, clique, arch);

            // Workspace-wide non-arch sets share the same weight (3.5x).
            assert_eq!(public, inter, "{:?}: public {} != inter {}", group, public, inter);
            assert_eq!(inter, clique, "{:?}: inter {} != clique {}", group, inter, clique);

            // intra_crate widens above the workspace-wide non-arch sets.
            assert!(intra >= public, "{:?}: intra {} < public {}", group, intra, public);

            // inner_crate is the widest (per-crate own architecture).
            assert!(inner >= intra, "{:?}: inner {} < intra {}", group, inner, intra);
        }
    }
}
