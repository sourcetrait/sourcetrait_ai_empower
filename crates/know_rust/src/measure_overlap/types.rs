use crate::*;

/// What: typed shape of the manual ground-truth JSON consumed by
/// measure-overlap. Mirrors the_user-validated per-target
/// architectural-protagonist lists at
/// notes/know_rust/manual_ground_truth.json.
///
/// Why: the_user 2026-06-04 ground-truth list is the calibration
/// target for the picker; measure-overlap reports the percentage of
/// ground-truth entries the picker surfaces, per target + aggregate.
///
/// Where: deserialized in `crate::measure_overlap::run::measure_overlap`
/// from the user-supplied JSON path.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct GroundTruthFile {
    pub targets: indexmap::IndexMap<String, TargetEntry>,
}

/// What: one target's ground-truth list plus an optional skip reason
/// for targets that lack a ground-truth list (container shapes,
/// submodule aggregators).
#[derive(serde::Deserialize, Debug, Clone, Default)]
pub struct TargetEntry {
    #[serde(default)]
    pub ground_truth: Vec<GroundTruthEntry>,
    #[serde(default)]
    pub skip_reason: Option<String>,
}

/// What: one ground-truth entry - human-readable canonical label plus
/// a list of case-sensitive substring aliases checked against picker
/// output kind:name strings.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct GroundTruthEntry {
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// What: per-section pick lists parsed from orientation.md's S5.1..5.6
/// sub-sections. Section keys mirror Python's measure_overlap.py
/// naming (`internals` is retained for S5.4 even after the phase 5a
/// rename to "clique" in the orientation header) so existing measured
/// outputs stay byte-equal during the conversion campaign.
#[derive(Debug, Clone, Default)]
pub struct PicksDict {
    pub architecture: Vec<String>,
    pub public: Vec<String>,
    pub inter_crate: Vec<String>,
    pub internals: Vec<String>,
    pub intra_crate: Vec<String>,
    pub inner_crate: Vec<String>,
}

impl PicksDict {
    /// What: union of all six pick lists in original section order.
    /// Used by the per-target match logic + the union-count column in
    /// the scoreboard table.
    pub fn all(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(
            self.architecture.len()
                + self.public.len()
                + self.inter_crate.len()
                + self.internals.len()
                + self.intra_crate.len()
                + self.inner_crate.len(),
        );
        out.extend(self.architecture.iter().cloned());
        out.extend(self.public.iter().cloned());
        out.extend(self.inter_crate.iter().cloned());
        out.extend(self.internals.iter().cloned());
        out.extend(self.intra_crate.iter().cloned());
        out.extend(self.inner_crate.iter().cloned());
        out
    }
}

/// What: per-ground-truth-entry match result - the canonical label,
/// the list of picker entries the aliases matched (deduped, original
/// order), and the boolean matched flag.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub label: String,
    pub matched_picks: Vec<String>,
    pub matched: bool,
}

/// What: per-target overlap measurement summary - per-section pick
/// counts + match list + overlap percentage. Consumed by the
/// scoreboard printer.
#[derive(Debug, Clone)]
pub struct TargetScore {
    pub n_total_picks_union: usize,
    pub n_ground_truth: usize,
    pub n_matched: usize,
    pub overlap_pct: f64,
    pub matches: Vec<MatchResult>,
}
