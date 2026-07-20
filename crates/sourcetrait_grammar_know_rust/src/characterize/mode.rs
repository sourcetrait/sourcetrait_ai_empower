use crate::*;

/// What: pick the histogram-driven trace mode (single_dominant /
/// co_equal_few / no_dominant) and escalate to `regional` when
/// workspace structure (multiple workspace_roots / multiple
/// connected components) calls for it. Returns the populated
/// `Selection` block written into fingerprint.json.
///
/// Why: characterize.py's `select_mode`. Drives downstream emit's
/// section selection; the ambiguous-band runner_up signal lets the
/// agent see when the choice was close to a boundary.
///
/// Where: called from `crate::characterize::run::characterize` after
/// `pattern_histogram` returns + connected components are known.
pub fn select_mode(
    ranked: &[(String, usize)],
    workspace_roots: &[String],
    n_components: usize,
    calibration: &Calibration,
) -> Selection {
    let total: usize = ranked.iter().map(|(_, c)| c).sum();
    let total = if total == 0 { 1 } else { total };
    let top_share = ranked.first().map(|(_, c)| *c as f64 / total as f64).unwrap_or(0.0);
    let second_share = ranked.get(1).map(|(_, c)| *c as f64 / total as f64).unwrap_or(0.0);
    let topk_share: f64 = ranked
        .iter()
        .take(calibration.mode.coequal_topk)
        .map(|(_, c)| *c as f64 / total as f64)
        .sum();

    let structural_regional = workspace_roots.len() > 1 || n_components > 1;
    let hist_mode = if top_share >= calibration.mode.dominance_share {
        "single_dominant"
    } else if topk_share >= calibration.mode.coequal_share
        && top_share < calibration.mode.dominance_share
    {
        "co_equal_few"
    } else {
        "no_dominant"
    };
    let mode = if structural_regional { "regional" } else { hist_mode };

    let mut runner_up: Option<String> = None;
    let mut notes: Vec<String> = Vec::new();
    if (top_share - calibration.mode.dominance_share).abs() <= calibration.mode.ambiguous_band {
        let r = if hist_mode != "single_dominant" {
            "single_dominant"
        } else {
            "co_equal_few"
        };
        runner_up = Some(r.to_string());
        notes.push(format!(
            "top_share={:.2} is within {} of the dominance threshold {}; mode is ambiguous between '{}' and '{}'. Confirm against the histogram before tracing.",
            top_share, calibration.mode.ambiguous_band, calibration.mode.dominance_share, hist_mode, r
        ));
    }
    if structural_regional && hist_mode != "no_dominant" {
        notes.push(format!(
            "structural signals (workspace_roots={}, components={}) forced 'regional', but the histogram alone would have chosen '{}'. The dominant pattern still holds within regions.",
            workspace_roots.len(),
            n_components,
            hist_mode
        ));
    }

    Selection {
        mode: mode.to_string(),
        histogram_mode: hist_mode.to_string(),
        structural_regional,
        runner_up,
        top_share: round3(top_share),
        second_share: round3(second_share),
        topk_share: round3(topk_share),
        notes,
    }
}

fn round3(x: f64) -> f64 {
    format!("{:.3}", x).parse().unwrap_or(x)
}
