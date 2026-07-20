use crate::*;

/// What: per-ground-truth-entry match check. For each entry, walk the
/// combined pick list and append any pick whose aliases (case-
/// sensitive substring) match; dedup the matched-picks list preserving
/// first occurrence.
///
/// Why: measure_overlap.py's `match_ground_truth` (lines 93-125). The
/// substring matcher lets the_user write loose ground-truth aliases
/// like `["Layout", "Constraint"]` and surface a match whenever the
/// picker emits `type_usage:Constraint::Length` etc.
///
/// Where: called by `crate::measure_overlap::score::score_target` after
/// `parse_picks` lifts the picker bullets out of orientation.md.
pub fn match_ground_truth(
    picks: &PicksDict,
    ground_truth: &[GroundTruthEntry],
) -> Vec<MatchResult> {
    let all_picks = picks.all();
    let mut results: Vec<MatchResult> = Vec::with_capacity(ground_truth.len());
    for gt in ground_truth {
        let mut matched_picks: Vec<String> = Vec::new();
        for pick in &all_picks {
            for alias in &gt.aliases {
                if pick.contains(alias) {
                    matched_picks.push(pick.clone());
                    break;
                }
            }
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut deduped: Vec<String> = Vec::new();
        for p in matched_picks {
            if seen.insert(p.clone()) {
                deduped.push(p);
            }
        }
        let matched = !deduped.is_empty();
        results.push(MatchResult {
            label: gt.label.clone(),
            matched_picks: deduped,
            matched,
        });
    }
    results
}

/// What: score one target: load orientation.md, parse picks, match
/// against the ground-truth list, return a TargetScore summary.
///
/// Why: measure_overlap.py's `score_target` (lines 128-153). Single-
/// target measurement; the main orchestrator iterates this across the
/// targets dict.
///
/// Where: called by `crate::measure_overlap::run::measure_overlap` per
/// target with the orientation path + the target's ground-truth list.
pub fn score_target(
    orientation_path: &Path,
    ground_truth: &[GroundTruthEntry],
) -> std::result::Result<TargetScore, Error> {
    let text = fs::read_to_string(orientation_path).map_err(|source| Error::Read {
        path: orientation_path.to_path_buf(),
        source,
    })?;
    let picks = parse_picks(&text);
    let matches = match_ground_truth(&picks, ground_truth);
    let n_total = ground_truth.len();
    let n_matched = matches.iter().filter(|m| m.matched).count();
    let pct = if n_total > 0 {
        100.0 * n_matched as f64 / n_total as f64
    } else {
        0.0
    };
    let union: HashSet<String> = picks.all().into_iter().collect();
    Ok(TargetScore {
        n_total_picks_union: union.len(),
        n_ground_truth: n_total,
        n_matched,
        overlap_pct: pct,
        matches,
    })
}
