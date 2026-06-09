use crate::*;

/// What: orchestrate the `know_rust measure demand` invocation. Scan
/// the consumer workspace in-process (items + usages walkers), load
/// the target's facts.json + fingerprint.json + orientation.md from
/// `target_out_dir`, build the demand report, print the scoreboard,
/// write `consumer_trace.json` (or `--out`), and return
/// `Error::DemandMisses` when any name or pair is uncovered (the
/// zero-miss bar, surfaced as a nonzero exit).
///
/// Why: the demand-side sibling of `measure overlap` - an in-house
/// consumer's actual usage of a target's internals is empirical
/// demand the pick sets must cover; any miss is a doc gap that would
/// force the consuming agent into a reference pull.
///
/// Where: dispatched by `crate::run::run` via the
/// `MeasureCommand::Demand` arm; called directly from
/// `tests/measure_demand_integration.rs`.
pub fn measure_demand(
    consumer_root: &Path,
    target_out_dir: &Path,
    out: Option<&Path>,
) -> std::result::Result<(), Error> {
    let consumer_items = scan_workspace(consumer_root);
    let consumer_usages = walk_workspace(consumer_root)?;

    let facts_path = target_out_dir.join("facts.json");
    let fp_path = target_out_dir.join("fingerprint.json");
    let orient_path = target_out_dir.join("orientation.md");
    let target_facts: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&facts_path).map_err(|source| Error::Read {
            path: facts_path.clone(),
            source,
        })?,
    )
    .map_err(|source| Error::Serialize { source })?;
    let target_fp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&fp_path).map_err(|source| Error::Read {
            path: fp_path.clone(),
            source,
        })?,
    )
    .map_err(|source| Error::Serialize { source })?;
    let orientation = fs::read_to_string(&orient_path).map_err(|source| Error::Read {
        path: orient_path.clone(),
        source,
    })?;

    let report = demand_report(
        &consumer_items,
        &consumer_usages,
        &target_facts,
        &target_fp,
        &orientation,
    );

    let consumer_canon =
        fs::canonicalize(consumer_root).unwrap_or_else(|_| consumer_root.to_path_buf());
    let target_canon =
        fs::canonicalize(target_out_dir).unwrap_or_else(|_| target_out_dir.to_path_buf());
    let s = &report.summary;
    println!("== demand scoreboard ==");
    println!("consumer: {}", consumer_canon.display());
    println!("target: {}", target_canon.display());
    println!();
    println!("{:<22} {:>6}", "demanded names", s.demanded_names);
    println!("{:<22} {:>6}", "covered", s.hits);
    println!("{:<22} {:>6}", "missing", s.miss_count);
    println!("{:<22} {:>6}", "module-namespace", s.mod_namespace_count);
    println!(
        "{:<22} {:>6}  (exact {}, name-level {}, missing {})",
        "pairs", s.pairs_total, s.pair_exact, s.pair_name_level, s.pair_miss_count
    );
    println!("{:<22} {:>6}", "globs", s.globs.len());
    if !s.misses.is_empty() {
        println!();
        println!("## missing names");
        for m in &s.misses {
            println!(
                "  [-] {} ({}) via {}",
                m.name,
                m.kinds.join(", "),
                m.srcs.join(", ")
            );
        }
    }
    if !s.pair_misses.is_empty() {
        println!();
        println!("## missing pairs");
        for p in &s.pair_misses {
            println!("  [-] {}", p);
        }
    }

    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| target_out_dir.join("consumer_trace.json"));
    let json =
        serde_json::to_string_pretty(&report).map_err(|source| Error::Serialize { source })?;
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: out_path.clone(),
        source,
    })?;
    eprintln!("[measure demand] wrote {}", out_path.display());

    if s.miss_count > 0 || s.pair_miss_count > 0 {
        return Err(Error::DemandMisses {
            names: s.miss_count,
            pairs: s.pair_miss_count,
        });
    }
    Ok(())
}
