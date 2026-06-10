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
    let consumer_renames = consumer_dep_renames(consumer_root);

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
        &consumer_renames,
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

/// What: collect the consumer's dependency renames
/// (`foo = { package = "bar" }`) from every Cargo.toml under the
/// consumer root into a binding -> package map (normalized binding
/// keys, first-wins across manifests in sorted path order).
///
/// Why: names are bindings - a consumer that renames the target
/// dependency writes `use foo::` for package `bar`, and the demand
/// trace must speak the consumer's own binding vocabulary. The map
/// is consumer-wide (not per-member) at first-wins granularity,
/// matching the trace's existing alias-map pragmatics.
///
/// Where: called by `measure_demand` before building the report;
/// threaded into `demand_report`'s root checks.
fn consumer_dep_renames(consumer_root: &Path) -> HashMap<String, String> {
    let mut manifests: Vec<PathBuf> = walkdir::WalkDir::new(consumer_root)
        .into_iter()
        .filter_entry(|e| !is_skip_dir(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.file_name() == "Cargo.toml")
        .map(|e| e.path().to_path_buf())
        .collect();
    manifests.sort();
    let mut renames: HashMap<String, String> = HashMap::new();
    for m in manifests {
        let text = match fs::read_to_string(&m) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let value: toml::Value = match toml::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        collect_dep_renames(&value, &mut renames);
    }
    renames
}

/// What: walk a parsed manifest value and record every
/// `<binding> = { package = "<pkg>" }` entry found in any table
/// whose key ends with `dependencies` (covers [dependencies],
/// dev/build variants, [workspace.dependencies], and
/// target-conditional tables via recursion).
///
/// Why: cargo accepts renames in all dependency table positions; a
/// structural walk keyed on the table-name suffix stays closed over
/// the manifest grammar without enumerating every position.
///
/// Where: called by `consumer_dep_renames` per manifest.
fn collect_dep_renames(value: &toml::Value, out: &mut HashMap<String, String>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, entry) in table {
        if key.ends_with("dependencies") {
            if let Some(deps) = entry.as_table() {
                for (binding, spec) in deps {
                    if let Some(pkg) = spec
                        .as_table()
                        .and_then(|t| t.get("package"))
                        .and_then(|p| p.as_str())
                    {
                        out.entry(binding.replace('-', "_"))
                            .or_insert_with(|| pkg.to_string());
                    }
                }
            }
        }
        collect_dep_renames(entry, out);
    }
}
