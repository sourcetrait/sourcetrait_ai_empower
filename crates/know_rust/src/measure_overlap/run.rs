use crate::*;

/// What: orchestrate the `know_rust measure-overlap` subcommand. Read
/// the manual ground-truth JSON, walk each target's orientation.md
/// under the sample directory, compute per-target + aggregate
/// overlap, and print the human-readable scoreboard to stdout.
///
/// Why: measure_overlap.py's `main()` (lines 156-222). The_user's
/// per-iteration measurement gate - reports how many architectural
/// protagonists the picker surfaces against the manual ground-truth
/// list per target + aggregate.
///
/// Where: dispatched by `crate::run::run` when the user invokes
/// `know_rust measure-overlap <samples_dir> <ground_truth_path>`.
pub fn measure_overlap(
    samples_dir: &Path,
    ground_truth_path: &Path,
) -> std::result::Result<(), Error> {
    if !samples_dir.is_dir() {
        return Err(Error::Read {
            path: samples_dir.to_path_buf(),
            source: io::Error::new(io::ErrorKind::NotFound, "samples_dir is not a directory"),
        });
    }
    if !ground_truth_path.is_file() {
        return Err(Error::Read {
            path: ground_truth_path.to_path_buf(),
            source: io::Error::new(io::ErrorKind::NotFound, "ground_truth file not found"),
        });
    }

    let gt_text = fs::read_to_string(ground_truth_path).map_err(|source| Error::Read {
        path: ground_truth_path.to_path_buf(),
        source,
    })?;
    let gt_data: GroundTruthFile = serde_json::from_str(&gt_text)
        .map_err(|source| Error::Serialize { source })?;

    let samples_canon = fs::canonicalize(samples_dir).unwrap_or_else(|_| samples_dir.to_path_buf());
    let gt_canon = fs::canonicalize(ground_truth_path).unwrap_or_else(|_| ground_truth_path.to_path_buf());

    println!("== overlap scoreboard ==");
    println!("sample: {}", samples_canon.display());
    println!("ground_truth: {}", gt_canon.display());
    println!();
    println!(
        "{:<22} {:>9}  {:>10}  {:>15}",
        "target", "overlap", "matched", "picks (union)"
    );
    println!("{}", "-".repeat(65));

    let mut target_names: Vec<String> = gt_data.targets.keys().cloned().collect();
    target_names.sort();

    let mut per_target: Vec<(String, TargetScore)> = Vec::new();
    let mut measurable_pct: Vec<f64> = Vec::new();
    for tname in &target_names {
        let tinfo = match gt_data.targets.get(tname) {
            Some(t) => t,
            None => continue,
        };
        let orient_path = samples_dir.join(tname).join("orientation.md");
        if !orient_path.is_file() {
            println!("{:<22} {:>40}", tname, "(no orientation.md)");
            continue;
        }
        if tinfo.ground_truth.is_empty() {
            let skip = tinfo
                .skip_reason
                .clone()
                .unwrap_or_else(|| "no ground truth list".to_string());
            println!("{:<22} {:>9}              -  {}", tname, "(skip)", skip);
            continue;
        }
        let result = score_target(&orient_path, &tinfo.ground_truth)?;
        println!(
            "{:<22} {:>8.1}% {:>4}/{:<4}  {:>15}",
            tname,
            result.overlap_pct,
            result.n_matched,
            result.n_ground_truth,
            result.n_total_picks_union
        );
        measurable_pct.push(result.overlap_pct);
        per_target.push((tname.clone(), result));
    }

    println!("{}", "-".repeat(65));
    if !measurable_pct.is_empty() {
        let avg = measurable_pct.iter().sum::<f64>() / measurable_pct.len() as f64;
        println!(
            "{:<22} {:>8.1}%   ({} measurable targets)",
            "AVERAGE",
            avg,
            measurable_pct.len()
        );
    }
    println!();

    println!("== per-target detail ==");
    for (target, entry) in &per_target {
        println!();
        println!(
            "## {} - {:.1}% ({}/{})",
            target, entry.overlap_pct, entry.n_matched, entry.n_ground_truth
        );
        for m in &entry.matches {
            let sym = if m.matched { "[+]" } else { "[-]" };
            let matched_str = if m.matched {
                let head: Vec<String> = m.matched_picks.iter().take(3).cloned().collect();
                let tail = if m.matched_picks.len() > 3 {
                    format!(" (+{} more)", m.matched_picks.len() - 3)
                } else {
                    String::new()
                };
                format!(" via {}{}", python_str_list_repr(&head), tail)
            } else {
                " MISSING".to_string()
            };
            println!("  {} {}{}", sym, m.label, matched_str);
        }
    }
    Ok(())
}

/// What: render a slice of strings using Python's list-of-strings
/// repr semantics - `['a', 'b', 'c']` with single quotes around each
/// element. Used for byte-equal scoreboard output vs the Python tool.
///
/// Why: Python's f-string of `list[:3]` yields the list's `__repr__`,
/// which single-quotes string elements. Rust's default Debug for
/// Vec<String> uses double quotes; this helper aligns the format.
///
/// Where: internal helper for the per-target detail print.
fn python_str_list_repr(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{}'", s)).collect();
    format!("[{}]", inner.join(", "))
}
