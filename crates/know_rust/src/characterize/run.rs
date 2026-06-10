use crate::*;

/// What: orchestrate the `know_rust characterize` invocation. Walk
/// Cargo.tomls, run scan items + scan usages in-process, build the
/// per-file items index, aggregate per-crate, compute pattern
/// histogram + select_mode + pattern_metrics + use-classification +
/// workspace_shape, and write `facts.json` + `fingerprint.json` to
/// the output directory.
///
/// Why: characterize.py's `main` rewritten as a pure rust orchestrator
/// that calls `scan_workspace` (items) and `walk_workspace` (usages)
/// directly rather than subprocessing the binary. The subprocess hop
/// disappears; intermediate `know_rust_items.json` +
/// `know_rust_usages.json` are still written to preserve sample
/// parity for downstream tooling that expects them.
///
/// Where: dispatched by `crate::run::run` via the
/// `Command::Characterize` clap variant; called from
/// `tests/characterize_integration.rs` when phase 6 lands.
pub fn characterize(
    workspace_root: &Path,
    out_dir: &Path,
    calibration: &Calibration,
) -> std::result::Result<(), Error> {
    fs::create_dir_all(out_dir).map_err(|source| Error::Write {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let WorkspaceDiscovery {
        crates,
        workspace_roots,
        units,
    } = find_crates(workspace_root);
    if crates.is_empty() {
        eprintln!(
            "[characterize] no Cargo packages found under {}",
            workspace_root.display()
        );
        return Ok(());
    }
    let components = compute_components(&crates);

    let item_facts = scan_workspace(workspace_root);
    {
        let path = out_dir.join("know_rust_items.json");
        let json = serde_json::to_string_pretty(&item_facts)
            .map_err(|source| Error::Serialize { source })?;
        fs::write(&path, json).map_err(|source| Error::Write {
            path,
            source,
        })?;
    }
    let items_by_file = build_items_index(&item_facts);

    let usage_facts = walk_workspace(workspace_root)?;
    {
        let path = out_dir.join("know_rust_usages.json");
        let json = serde_json::to_string_pretty(&usage_facts)
            .map_err(|source| Error::Serialize { source })?;
        fs::write(&path, json).map_err(|source| Error::Write {
            path,
            source,
        })?;
    }

    let mut all_facts = WorkspaceFacts {
        impls: Vec::new(),
        traits: Vec::new(),
        types: Vec::new(),
        fns: Vec::new(),
        uses: Vec::new(),
        macros: Vec::new(),
        derives: Vec::new(),
        macro_defs: Vec::new(),
        type_usages: Vec::new(),
        mods: Vec::new(),
        example_type_usages: Vec::new(),
        seams: indexmap::IndexMap::new(),
        ast_type_refs: Vec::new(),
        ast_method_refs: Vec::new(),
        carries: BTreeMap::new(),
        pair_aliases: BTreeMap::new(),
    };
    let mut per_crate: indexmap::IndexMap<String, PerCrateFingerprint> = indexmap::IndexMap::new();
    let mut free_fns_by_crate: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();

    let crate_dirs: indexmap::IndexMap<String, String> = crates
        .iter()
        .map(|(k, v)| (k.clone(), v.dir.clone()))
        .collect();
    let cfg_test_skips = collect_cfg_test_module_skips(workspace_root);
    for (name, info) in &crates {
        let cf = scan_crate(
            workspace_root,
            name,
            &info.dir,
            &crate_dirs,
            &items_by_file,
            &cfg_test_skips,
        );
        per_crate.insert(
            name.clone(),
            PerCrateFingerprint {
                dir: info.dir.clone(),
                sloc: cf.sloc,
                deps: info.deps.clone(),
                n_impls: cf.impls.len(),
                n_types: cf.types.len(),
                n_traits: cf.traits.len(),
                n_fns: cf.fns.len(),
                seams: indexmap::IndexMap::new(),
                version: info.version.clone(),
                lib_name: info.lib_name.clone(),
                unit: info.unit.clone(),
            },
        );
        let free_fns_count = cf
            .fns
            .iter()
            .filter(|f| f.get("brace_depth").and_then(|v| v.as_u64()).unwrap_or(99) == 0)
            .count();
        free_fns_by_crate.insert(name.clone(), free_fns_count);

        extend_with_crate(&mut all_facts.impls, cf.impls, name);
        extend_with_crate(&mut all_facts.traits, cf.traits, name);
        extend_with_crate(&mut all_facts.types, cf.types, name);
        extend_with_crate(&mut all_facts.fns, cf.fns, name);
        extend_with_crate(&mut all_facts.uses, cf.uses, name);
        extend_with_crate(&mut all_facts.macros, cf.macros, name);
        extend_with_crate(&mut all_facts.derives, cf.derives, name);
        extend_with_crate(&mut all_facts.macro_defs, cf.macro_defs, name);
        extend_with_crate(&mut all_facts.mods, cf.mods, name);
        extend_with_crate(&mut all_facts.type_usages, cf.type_usages, name);
        extend_with_crate(&mut all_facts.example_type_usages, cf.example_type_usages, name);
    }
    for (k, v) in &item_facts.seams {
        *all_facts.seams.entry(k.clone()).or_default() += v;
    }
    // R2: propagate carries from the items walker to the workspace
    // facts. The per-crate split is not applied here because carry
    // keys are already pattern-scoped (group:name) and the picked
    // item's crate is recoverable via the picker's existing
    // pattern_metrics::defining_crate lookup.
    for (pat, entries) in &item_facts.carries {
        all_facts
            .carries
            .entry(pat.clone())
            .or_default()
            .extend(entries.iter().cloned());
    }

    // Workspace-origin gate on carry: the picks-data design rule is that
    // all forms of picks (Picked + Carried) exclude types defined outside
    // the workspace. The items walker records carry structurally and has
    // no cross-workspace knowledge; here the full workspace type+trait set
    // exists to enforce origin. See notes/know_rust/working/02_picks_data.md.
    filter_carries_to_workspace(&mut all_facts);

    for ent in &usage_facts.ast_fn_sig_usages {
        if ent.ident.is_empty() {
            continue;
        }
        let using_crate = resolve_crate_for_file(&ent.file, &crate_dirs);
        if using_crate.is_empty() {
            continue;
        }
        all_facts.ast_type_refs.push(serde_json::json!({
            "name": ent.ident,
            "file": ent.file,
            "line": ent.line,
            "crate": using_crate,
            "source": "fn_sig_usages",
            "qualifier": ent.qualifier,
        }));
    }
    for ent in &usage_facts.ast_field_usages {
        if ent.ident.is_empty() {
            continue;
        }
        let using_crate = resolve_crate_for_file(&ent.file, &crate_dirs);
        if using_crate.is_empty() {
            continue;
        }
        all_facts.ast_type_refs.push(serde_json::json!({
            "name": ent.ident,
            "file": ent.file,
            "line": ent.line,
            "crate": using_crate,
            "source": "field_usages",
            "qualifier": ent.qualifier,
        }));
    }
    for ent in &usage_facts.ast_type_alias_usages {
        if ent.ident.is_empty() {
            continue;
        }
        let using_crate = resolve_crate_for_file(&ent.file, &crate_dirs);
        if using_crate.is_empty() {
            continue;
        }
        all_facts.ast_type_refs.push(serde_json::json!({
            "name": ent.ident,
            "file": ent.file,
            "line": ent.line,
            "crate": using_crate,
            "source": "type_alias_usages",
            "qualifier": ent.qualifier,
        }));
    }
    for ent in &usage_facts.ast_method_ref_usages {
        if ent.outer.is_empty() || ent.inner.is_empty() {
            continue;
        }
        let using_crate = resolve_crate_for_file(&ent.file, &crate_dirs);
        if using_crate.is_empty() {
            continue;
        }
        all_facts.ast_method_refs.push(serde_json::json!({
            "name": format!("{}::{}", ent.outer, ent.inner),
            "outer": ent.outer,
            "inner": ent.inner,
            "file": ent.file,
            "line": ent.line,
            "container": ent.container,
            "crate": using_crate,
            "qualifier": ent.qualifier,
        }));
    }

    let (ranked, by_kind, reg_calls) = pattern_histogram(&all_facts, &free_fns_by_crate);
    let selection = select_mode(&ranked, &workspace_roots, components.len(), calibration);

    let total_sloc: usize = per_crate.values().map(|c| c.sloc).sum::<usize>().max(1);
    let seam_total: usize = all_facts.seams.values().sum();
    let seam_density = seam_total as f64 / (total_sloc as f64 / 1000.0);

    let mut example_rs_files = 0usize;
    for entry in walkdir::WalkDir::new(workspace_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let rel = match entry.path().strip_prefix(workspace_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.components().any(|c| c.as_os_str() == "examples") {
            example_rs_files += 1;
        }
    }

    let mut pattern_metrics =
        compute_pattern_metrics(&all_facts, Some(&usage_facts), &crates, calibration);
    // Declaration-driven API channel: pub-reachable module-level fns
    // get a pair key even with zero usage; alternate binding outers
    // land in facts.pair_aliases for the demand matcher.
    all_facts.pair_aliases = decl_api_channel(&all_facts, &crates, &mut pattern_metrics);
    let workspace_use_classification =
        classify_workspace_use(&crates, &pattern_metrics, calibration);

    let totals = Totals {
        crates: crates.len(),
        sloc: total_sloc,
        impls: all_facts.impls.len(),
        types: all_facts.types.len(),
        traits: all_facts.traits.len(),
        fns: all_facts.fns.len(),
        example_rs_files,
    };

    let thresholds = Thresholds {
        dominance_share: calibration.mode.dominance_share,
        coequal_topk: calibration.mode.coequal_topk,
        coequal_share: calibration.mode.coequal_share,
        ambiguous_band: calibration.mode.ambiguous_band,
        seam_dense_per_kloc: calibration.mode.seam_dense_per_kloc,
        note: "Declared defaults, not validated constants. Override via ORIENT_* env vars. The full histogram is reported so the choice is auditable.".to_string(),
    };

    let workspace_units: indexmap::IndexMap<String, WorkspaceUnitFingerprint> = units
        .iter()
        .map(|(k, u)| {
            let (url, rev) = match &u.provenance {
                UnitProvenance::Submodule { url, rev } => (Some(url.clone()), rev.clone()),
                _ => (None, None),
            };
            (
                k.clone(),
                WorkspaceUnitFingerprint {
                    root_dir: u.root_dir.clone(),
                    provenance: UnitProvenanceWire {
                        kind: u.provenance.wire_kind().to_string(),
                        url,
                        rev,
                    },
                    members: u.members.clone(),
                    populated: u.populated,
                },
            )
        })
        .collect();

    let mut fp = Fingerprint {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        repo_root: workspace_root.display().to_string(),
        totals,
        workspace_roots,
        workspace_units,
        components,
        n_components: 0,
        pattern_histogram: ranked
            .iter()
            .take(40)
            .map(|(p, c)| PatternHistogramEntry { pattern: p.clone(), count: *c })
            .collect(),
        pattern_by_kind: by_kind,
        registration_macros: reg_calls,
        seam_inventory: all_facts.seams.clone(),
        seam_density_per_kloc: format!("{:.2}", seam_density).parse().unwrap_or(seam_density),
        selection,
        pattern_metrics,
        workspace_use_classification,
        thresholds,
        per_crate: per_crate.clone(),
        workspace_shape: WorkspaceShape {
            shape: "monolith".to_string(),
            signals: indexmap::IndexMap::new(),
            reasoning: String::new(),
        },
    };
    fp.n_components = fp.components.len();
    fp.workspace_shape = classify_workspace_shape(&per_crate, &all_facts);

    {
        let path = out_dir.join("fingerprint.json");
        let json = serde_json::to_string_pretty(&fp)
            .map_err(|source| Error::Serialize { source })?;
        fs::write(&path, json).map_err(|source| Error::Write {
            path,
            source,
        })?;
    }
    {
        let path = out_dir.join("facts.json");
        let json = serde_json::to_string_pretty(&all_facts)
            .map_err(|source| Error::Serialize { source })?;
        fs::write(&path, json).map_err(|source| Error::Write {
            path,
            source,
        })?;
    }

    eprintln!(
        "[characterize] {} crates, {} SLOC, {} component(s), {} workspace root(s)",
        crates.len(),
        total_sloc,
        fp.n_components,
        fp.workspace_roots.len()
    );
    eprintln!(
        "[characterize] mode = {}  (histogram: {}, top_share={})",
        fp.selection.mode, fp.selection.histogram_mode, fp.selection.top_share
    );
    Ok(())
}

fn extend_with_crate(
    dst: &mut Vec<serde_json::Value>,
    src: Vec<serde_json::Value>,
    crate_name: &str,
) {
    for mut v in src {
        if let serde_json::Value::Object(ref mut map) = v {
            map.insert(
                "crate".to_string(),
                serde_json::Value::String(crate_name.to_string()),
            );
        }
        dst.push(v);
    }
}

/// What: drop carry keys + carried names that are not workspace-declared,
/// per the picks-data design rule that all forms of picks (Picked +
/// Carried) exclude types defined outside the workspace (see
/// notes/know_rust/working/02_picks_data.md).
///
/// Why: the items walker records carry structurally per-file and cannot
/// tell a workspace type from std / an external crate (`impl Trait for
/// Vec<T>` yields `structure:Vec`; a field `Vec<Bar>` carries `Vec`).
/// characterize is where the full workspace type+trait set is known, so
/// the origin gate lives here rather than as a hardcoded skip-list in the
/// walker (a list misses externals not on it, e.g. `RangeFrom`).
///
/// Where: called from `characterize` after per-crate facts + the
/// item-walker carries are merged into `all_facts`.
fn filter_carries_to_workspace(all_facts: &mut WorkspaceFacts) {
    let ws_types: HashSet<String> = all_facts
        .types
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let ws_traits: HashSet<String> = all_facts
        .traits
        .iter()
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let name_ok = |n: &str| ws_types.contains(n) || ws_traits.contains(n);
    let key_ok = |pat: &Pattern| -> bool {
        match pat {
            Pattern::Structure(name) => ws_types.contains(name),
            // Configuring carry keys are derive-sourced (the walker's
            // derive site records the derived trait name); gate on the
            // workspace trait set. Attr-macro configuring patterns have
            // no carry site, so they never appear here.
            Pattern::Traits(name) | Pattern::Configuring(name) => ws_traits.contains(name),
            Pattern::ImplementationFunctions { outer, .. }
            | Pattern::TraitFunctions { outer, .. } => {
                outer.as_str() == "_"
                    || ws_types.contains(outer.as_str())
                    || ws_traits.contains(outer.as_str())
            }
            Pattern::Utilities(_) | Pattern::Globals(_) => false,
        }
    };
    let mut filtered: BTreeMap<Pattern, Vec<CarryEntry>> = BTreeMap::new();
    for (pat, entries) in &all_facts.carries {
        if !key_ok(pat) {
            continue;
        }
        let kept: Vec<CarryEntry> = entries
            .iter()
            .filter(|e| name_ok(&e.name))
            .cloned()
            .collect();
        if !kept.is_empty() {
            filtered.insert(pat.clone(), kept);
        }
    }
    all_facts.carries = filtered;
}
