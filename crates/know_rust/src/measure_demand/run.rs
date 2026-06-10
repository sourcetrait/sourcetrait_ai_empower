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
    let report = trace_pair(consumer_root, target_out_dir)?;

    let consumer_canon =
        fs::canonicalize(consumer_root).unwrap_or_else(|_| consumer_root.to_path_buf());
    let target_canon =
        fs::canonicalize(target_out_dir).unwrap_or_else(|_| target_out_dir.to_path_buf());
    println!("== demand scoreboard ==");
    println!("consumer: {}", consumer_canon.display());
    println!("target: {}", target_canon.display());
    println!();
    print_summary(&report.summary);

    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| target_out_dir.join("consumer_trace.json"));
    write_report(&report, &out_path)?;

    let s = &report.summary;
    if s.miss_count > 0 || s.pair_miss_count > 0 {
        return Err(Error::DemandMisses {
            names: s.miss_count,
            pairs: s.pair_miss_count,
        });
    }
    Ok(())
}

/// What: run the demand trace for one (consumer root, target pass
/// dir) pair: in-process consumer scans, target artifact loads, and
/// the report build. No printing, no gating, no writes.
///
/// Why: the single-pair CLI and the roster-driven batch share
/// exactly this unit; extracting it keeps the gate + presentation
/// concerns in the orchestrators.
///
/// Where: called by `measure_demand` and per-row by
/// `measure_consumers`.
pub(crate) fn trace_pair(
    consumer_root: &Path,
    target_out_dir: &Path,
) -> std::result::Result<DemandReport, Error> {
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

    Ok(demand_report(
        &consumer_items,
        &consumer_usages,
        &consumer_renames,
        &target_facts,
        &target_fp,
        &orientation,
    ))
}

/// What: print one summary block (counts + miss details) to stdout.
///
/// Why: shared between the single-pair scoreboard and the batch's
/// per-pair sections so the human-readable shape stays identical.
///
/// Where: called by `measure_demand` and `measure_consumers`.
fn print_summary(s: &DemandSummary) {
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
}

/// What: serialize a report to pretty JSON at `out_path`.
fn write_report(report: &DemandReport, out_path: &Path) -> std::result::Result<(), Error> {
    let json =
        serde_json::to_string_pretty(report).map_err(|source| Error::Serialize { source })?;
    fs::write(out_path, json).map_err(|source| Error::Write {
        path: out_path.to_path_buf(),
        source,
    })?;
    eprintln!("[measure demand] wrote {}", out_path.display());
    Ok(())
}

/// What: orchestrate the roster-driven batch: parse
/// consumer_repos.txt, resolve each row's consumer root + target
/// pass dir, trace every pair, write each pair's trace JSON into its
/// pair dir, print per-pair summaries + an aggregate table, emit the
/// aggregated consumer-demand weight blob from Weight-role rows when
/// `--weights-out` is given, and gate (nonzero exit) on misses from
/// Audit-role rows only.
///
/// Why: the four-plus-pair re-audit is one command instead of N, the
/// role column keeps weight sources out of the independent audit,
/// and the blob regeneration is a whole-roster pure function per the
/// weights design.
///
/// Where: dispatched by `crate::run::run` via the
/// `MeasureCommand::Consumers` arm; called from
/// `tests/measure_consumers_integration.rs`.
pub fn measure_consumers(
    consumer_repos: &Path,
    pairs_root: &Path,
    outputs_root: &Path,
    weights_out: Option<&Path>,
) -> std::result::Result<(), Error> {
    let rows = parse_consumer_repos(consumer_repos)?;
    let mut blob = WeightBlob::default();
    let mut audit_name_misses = 0usize;
    let mut audit_pair_misses = 0usize;
    let mut table: Vec<String> = Vec::new();

    for row in &rows {
        let pair_dir = pairs_root.join(&row.snake);
        let consumer_root = resolve_consumer_root(row, &pair_dir);
        let pass = {
            let own = pair_dir.join(format!("orientation_{}", row.target));
            if own.is_dir() {
                own
            } else {
                outputs_root.join(&row.target)
            }
        };
        if !consumer_root.is_dir() {
            return Err(Error::Read {
                path: consumer_root,
                source: io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("consumer root for `{}` not found", row.snake),
                ),
            });
        }
        println!(
            "== {} -> {} ({}) ==",
            row.snake,
            row.target,
            row.role.wire()
        );
        let report = trace_pair(&consumer_root, &pass)?;
        print_summary(&report.summary);
        println!();
        let trace_path = pair_dir.join(format!("consumer_trace_{}.json", row.snake));
        if pair_dir.is_dir() {
            write_report(&report, &trace_path)?;
        }
        let s = &report.summary;
        table.push(format!(
            "{:<14} {:<14} {:<7} names {:>4}/{:<4} pairs {:>4}/{:<4} miss {}+{}",
            row.target,
            row.snake,
            row.role.wire(),
            s.hits,
            s.hits + s.miss_count,
            s.pair_exact + s.pair_name_level,
            s.pairs_total,
            s.miss_count,
            s.pair_miss_count,
        ));
        match row.role {
            ConsumerRole::Audit => {
                audit_name_misses += s.miss_count;
                audit_pair_misses += s.pair_miss_count;
            }
            ConsumerRole::Weight => {
                fold_weights(&mut blob, &row.target, &row.snake, &pass, &report);
            }
        }
    }

    println!("== aggregate ==");
    for line in &table {
        println!("{}", line);
    }

    if let Some(wpath) = weights_out {
        let json = serde_json::to_string_pretty(&blob)
            .map_err(|source| Error::Serialize { source })?;
        fs::write(wpath, json).map_err(|source| Error::Write {
            path: wpath.to_path_buf(),
            source,
        })?;
        eprintln!("[measure consumers] wrote weights {}", wpath.display());
    }

    if audit_name_misses > 0 || audit_pair_misses > 0 {
        return Err(Error::DemandMisses {
            names: audit_name_misses,
            pairs: audit_pair_misses,
        });
    }
    Ok(())
}

/// What: resolve a roster row's consumer root: the `local:<path>`
/// URL form, then a `# root:` override, then the pair dir's clone
/// (URL basename minus `.git`).
///
/// Why: three declaration shapes cover the roster's realities -
/// on-box workspace crates, reused clones living elsewhere
/// (cosmic-files inside the cosmic-epoch submodule), and ordinary
/// pair-dir clones.
///
/// Where: called per row by `measure_consumers`.
fn resolve_consumer_root(row: &ConsumerRow, pair_dir: &Path) -> PathBuf {
    if let Some(local) = row.url.strip_prefix("local:") {
        return PathBuf::from(local);
    }
    if let Some(root) = &row.root_override {
        return root.clone();
    }
    let basename = row
        .url
        .rsplit('/')
        .next()
        .unwrap_or(&row.snake)
        .trim_end_matches(".git");
    pair_dir.join(basename)
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
/// Where: called by `trace_pair` before building the report.
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
