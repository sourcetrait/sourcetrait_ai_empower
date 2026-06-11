use crate::*;

/// What: compute the per-pattern metrics dictionary: for each pattern
/// emitted by the histogram (plus synthesized pub_type + method_ref
/// family entries from the AST signal), record the defining crate,
/// intra/inter counts, inter_ratio, is_pub flag, and example /
/// curated_example counts.
///
/// Why: characterize.py's `_compute_pattern_metrics` is what drives
/// emit's significance-set scoring (architecture / public /
/// inter_crate / clique / intra_crate / inner_crate). The defining
/// crate lookup chain (type -> module -> crate-as-namespace) closes
/// real-workspace patterns the simpler regex/text scan would miss.
///
/// Where: called from `crate::characterize::run::characterize` after
/// `all_facts` is populated and the AST scan returns.
pub fn compute_pattern_metrics(
    all_facts: &WorkspaceFacts,
    ast_usages: Option<&UsageFacts>,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    calibration: &Calibration,
) -> indexmap::IndexMap<String, PatternMetric> {
    let mut metrics: indexmap::IndexMap<String, PatternMetric> = indexmap::IndexMap::new();

    let impl_target_count = build_impl_target_count(&all_facts.impls);
    let (type_def_lookup, type_visibility) =
        build_type_lookup(&all_facts.types, &impl_target_count);
    let trait_def_lookup = build_trait_lookup(&all_facts.traits);
    let macro_def_lookup = build_macro_def_lookup(&all_facts.macro_defs);
    let mod_def_lookup = build_mod_lookup(&all_facts.mods);
    let crate_name_lookup = build_crate_name_lookup(all_facts);

    // Item path resolution (the assumed mode of attribution; R8
    // slice 2, working/02): per-file import maps + the workspace
    // member set + the full name->declaring-crates map (for prelude
    // shadowing and same-crate preference). The import surface is
    // built through the shared resolution module so capture and
    // demand (measure_demand) read imports identically.
    let import_maps = build_import_bindings(all_facts.uses.iter().filter_map(|u| {
        Some((
            u.get("file").and_then(|v| v.as_str())?,
            u.get("path").and_then(|v| v.as_str())?,
        ))
    }));
    // Bindings -> identity vocabulary: package names + lib renames
    // globally, dependency renames per consuming crate.
    let vocab = ResolveVocab::from_crates(crates);
    let mut local_decl_crates: std::collections::HashMap<
        String,
        std::collections::HashSet<String>,
    > = std::collections::HashMap::new();
    for list in [&all_facts.types, &all_facts.traits] {
        for t in list.iter() {
            if let (Some(n), Some(c)) = (
                t.get("name").and_then(|v| v.as_str()),
                t.get("crate").and_then(|v| v.as_str()),
            ) {
                local_decl_crates
                    .entry(n.to_string())
                    .or_default()
                    .insert(c.to_string());
            }
        }
    }
    let enum_names: std::collections::HashSet<String> = all_facts
        .types
        .iter()
        .filter(|t| t.get("kind").and_then(|v| v.as_str()) == Some("enum"))
        .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
        .collect();
    // mod-name -> declaring crates: a path ROOT that is neither a
    // keyword, a workspace member, nor import-resolved is crate-local
    // only when the using crate actually declares a module of that
    // name; otherwise it is an external crate root.
    let mut local_mod_crates: std::collections::HashMap<
        String,
        std::collections::HashSet<String>,
    > = std::collections::HashMap::new();
    for m in &all_facts.mods {
        if let (Some(n), Some(c)) = (
            m.get("name").and_then(|v| v.as_str()),
            m.get("crate").and_then(|v| v.as_str()),
        ) {
            local_mod_crates
                .entry(n.to_string())
                .or_default()
                .insert(c.to_string());
        }
    }
    // crate -> re-exported names (pub use leaves): a workspace FACADE
    // crate re-exporting another member's item is the same item -
    // sites importing through the facade (use ratatui::Frame, decl in
    // ratatui-core) credit the declaring crate. Name-level match,
    // consistent with the system's attribution granularity.
    let facades = build_facade_index(
        all_facts.uses.iter().filter_map(|u| {
            if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
                return None;
            }
            Some((
                u.get("crate").and_then(|v| v.as_str())?,
                u.get("path").and_then(|v| v.as_str())?,
            ))
        }),
        &vocab,
    );

    let test_weight = calibration.picker.example.test_weight;
    let bench_weight = calibration.picker.example.bench_weight;
    // Example evidence rides the same origin rule as usage counting:
    // a site whose path resolves to std / an external crate is not
    // evidence of the workspace pattern (std::env::args in an
    // examples/ file must not credit a workspace `env` mod).
    let gated_example_usages: Vec<serde_json::Value> = all_facts
        .example_type_usages
        .iter()
        .filter(|tu| {
            let name = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let file = tu.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let using = tu.get("crate").and_then(|v| v.as_str()).unwrap_or("");
            let qualifier = tu.get("qualifier").and_then(|v| v.as_str());
            let outer = name.split("::").next().unwrap_or(name);
            let origin = resolve_site_origin(
                file,
                outer,
                qualifier,
                using,
                &import_maps,
                &vocab,
                &local_decl_crates,
                &local_mod_crates,
            );
            !matches!(origin, IdentOrigin::Std | IdentOrigin::External)
        })
        .cloned()
        .collect();
    let (example_files_by_name, curated_example_count_by_name) =
        compute_example_counts(&gated_example_usages, test_weight, bench_weight);

    let kinds: &[(&str, &dyn Fn(&serde_json::Value) -> Option<String>)] = &[
        ("trait_impl", &|f: &serde_json::Value| {
            if f.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                return None;
            }
            f.get("trait").and_then(|v| v.as_str()).map(String::from)
        }),
        ("derive", &|f: &serde_json::Value| {
            f.get("trait").and_then(|v| v.as_str()).map(String::from)
        }),
        ("type_usage", &|f: &serde_json::Value| {
            f.get("name").and_then(|v| v.as_str()).map(String::from)
        }),
        ("reg_macro", &|f: &serde_json::Value| {
            if f.get("kind").and_then(|v| v.as_str()) != Some("macro_invocation") {
                return None;
            }
            f.get("name").and_then(|v| v.as_str()).map(String::from)
        }),
        ("attr_macro", &|f: &serde_json::Value| {
            if f.get("kind").and_then(|v| v.as_str()) != Some("attr_macro") {
                return None;
            }
            f.get("name").and_then(|v| v.as_str()).map(String::from)
        }),
    ];
    let sources_by_kind: indexmap::IndexMap<&str, &[serde_json::Value]> = indexmap::IndexMap::from([
        ("trait_impl", all_facts.impls.as_slice()),
        ("derive", all_facts.derives.as_slice()),
        ("type_usage", all_facts.type_usages.as_slice()),
        ("reg_macro", all_facts.macros.as_slice()),
        ("attr_macro", all_facts.macros.as_slice()),
    ]);

    let mut seen_patterns: indexmap::IndexSet<(String, String)> = indexmap::IndexSet::new();
    for (kind, _inner_fn) in kinds {
        let source = match sources_by_kind.get(kind) {
            Some(s) => *s,
            None => continue,
        };
        let inner_fn = kinds.iter().find(|(k, _)| k == kind).map(|(_, f)| *f).unwrap();
        for fact in source {
            if let Some(inner) = inner_fn(fact) {
                seen_patterns.insert((kind.to_string(), inner));
            }
        }
    }
    for tu in &all_facts.example_type_usages {
        if let Some(name) = tu.get("name").and_then(|v| v.as_str()) {
            seen_patterns.insert(("type_usage".to_string(), name.to_string()));
        }
    }

    for (kind, inner) in &seen_patterns {
        let pattern = format!("{}:{}", kind, inner);
        if should_skip_pattern(&pattern, calibration) {
            continue;
        }
        let source = match sources_by_kind.get(kind.as_str()) {
            Some(s) => *s,
            None => &[][..],
        };
        // Broad-channels fill (the_user 2026-06-09; working/02 "Design
        // rule: broad channels, mechanically filled"): non-type_usage
        // kinds derive example evidence from the example-dir file paths
        // of their own matching facts. type_usage keeps the
        // example_type_usages partition (its example occurrences are
        // evidence-only and never enter the usage stream).
        let ex_files = example_file_count(kind, inner, source);
        let (example_count_v, curated_v) = if kind == "type_usage" {
            (
                example_count_value(kind, inner, &example_files_by_name),
                curated_example_count(kind, inner, &curated_example_count_by_name),
            )
        } else {
            (serde_json::Value::from(ex_files as f64), ex_files)
        };
        let defn = pattern_def(
            kind,
            inner,
            &type_def_lookup,
            &trait_def_lookup,
            &macro_def_lookup,
            &mod_def_lookup,
            &crate_name_lookup,
        );
        if defn.is_none() {
            metrics.insert(
                pattern.clone(),
                PatternMetric {
                    defining_crate: None,
                    intra_count: 0,
                    inter_count: 0,
                    inter_ratio: 0.0,
                    is_pub: false,
                    example_count: example_count_v,
                    curated_example_count: curated_v,
                    sub_form: None,
                },
            );
            continue;
        }
        let (defn, defn_source) = defn.unwrap();
        let defining_crate = defn.crate_name.clone();
        let mut is_pub = defn.visibility.starts_with("pub");
        if !is_pub && defn.macro_exported {
            is_pub = true;
        }
        // Per-site resolution gate: a fact only credits the pattern
        // when its identifier resolves to the declaring crate (or is
        // an unresolved crate-local fallback). Std / external /
        // other-workspace resolutions were never candidates - this is
        // the workspace-origin rule holding at usage sites, not a
        // separate exclusion (working/02).
        let resolve_target: String = match kind.as_str() {
            "type_usage" => inner.split("::").next().unwrap_or(inner).to_string(),
            _ => inner.clone(),
        };
        let mut intra = 0usize;
        let mut inter = 0usize;
        let mut gated_example_files: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for fact in source {
            if !pattern_match(kind, inner, fact) {
                continue;
            }
            let using = match fact.get("crate").and_then(|v| v.as_str()) {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };
            let file = fact.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let origin = resolve_site_origin(
                file,
                &resolve_target,
                fact.get("qualifier").and_then(|v| v.as_str()),
                using,
                &import_maps,
                &vocab,
                &local_decl_crates,
                &local_mod_crates,
            );
            if !site_credits(&origin, using, &defining_crate, &resolve_target, &facades) {
                continue;
            }
            let norm = file.replace('\\', "/");
            // Example-dir sites are curated EVIDENCE, not usage:
            // credit the evidence tally and skip the intra/inter
            // increment. type_usage already arrives example-free here
            // (the example_type_usages partition); trait_impl /
            // derive / reg_macro / attr_macro align to it.
            if norm.contains("/examples/") || norm.starts_with("examples/") {
                gated_example_files.insert(norm);
                continue;
            }
            if using == defining_crate {
                intra += 1;
            } else {
                inter += 1;
            }
        }
        // Mod / crate-namespace defn fallbacks with zero credited
        // usage are resolution artifacts (env::args class), not
        // picks; type-def-backed patterns may legitimately be
        // example-only.
        if kind == "type_usage"
            && matches!(defn_source, DefSource::Mod | DefSource::CrateName)
            && intra + inter == 0
        {
            continue;
        }
        let (example_count_v, curated_v) = if kind == "type_usage" {
            (example_count_v, curated_v)
        } else {
            (
                serde_json::Value::from(gated_example_files.len() as f64),
                gated_example_files.len(),
            )
        };
        let total = intra + inter;
        let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
        metrics.insert(
            pattern.clone(),
            PatternMetric {
                defining_crate: Some(defining_crate),
                intra_count: intra,
                inter_count: inter,
                inter_ratio: round3(ratio),
                is_pub,
                example_count: example_count_v,
                curated_example_count: curated_v,
                sub_form: None,
            },
        );
    }

    let _ = type_visibility;

    if let Some(usages) = ast_usages {
        let crate_dirs: indexmap::IndexMap<String, String> = crates
            .iter()
            .map(|(k, v)| (k.clone(), v.dir.clone()))
            .collect();

        let mut ast_by_ident: indexmap::IndexMap<String, Vec<(String, Option<String>)>> =
            indexmap::IndexMap::new();
        for e in &usages.ast_fn_sig_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push((e.file.clone(), e.qualifier.clone()));
            }
        }
        for e in &usages.ast_field_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push((e.file.clone(), e.qualifier.clone()));
            }
        }
        for e in &usages.ast_type_alias_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push((e.file.clone(), e.qualifier.clone()));
            }
        }

        for (ident, hits) in ast_by_ident {
            if ident.is_empty() {
                continue;
            }
            let defn = type_def_lookup
                .get(&ident)
                .or_else(|| trait_def_lookup.get(&ident));
            let defn = match defn {
                Some(d) => d,
                None => continue,
            };
            if !defn.visibility.starts_with("pub") {
                continue;
            }
            let pattern = format!("pub_type:{}", ident);
            if should_skip_pattern(&pattern, calibration) {
                continue;
            }
            if metrics.contains_key(&pattern) {
                continue;
            }
            let defining_crate = defn.crate_name.clone();
            // Resolution gate (R8 slice 2): only sites whose ident
            // resolves to the declaring crate (or unresolved
            // crate-local fallback) credit the synthesized pattern.
            let credited: Vec<String> = hits
                .iter()
                .filter(|(f, q)| {
                    let using = resolve_crate_for_file(f, &crate_dirs);
                    let origin = resolve_site_origin(
                        f,
                        &ident,
                        q.as_deref(),
                        &using,
                        &import_maps,
                        &vocab,
                        &local_decl_crates,
                        &local_mod_crates,
                    );
                    site_credits(&origin, &using, &defining_crate, &ident, &facades)
                })
                .map(|(f, _)| f.clone())
                .collect();
            if credited.is_empty() {
                continue;
            }
            let (intra, inter, example_count, curated_count) =
                count_usages(&credited, &defining_crate, &crate_dirs);
            let total = intra + inter;
            let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
            metrics.insert(
                pattern,
                PatternMetric {
                    defining_crate: Some(defining_crate),
                    intra_count: intra,
                    inter_count: inter,
                    inter_ratio: round3(ratio),
                    is_pub: true,
                    example_count: serde_json::Value::from(example_count as f64),
                    curated_example_count: curated_count,
                    sub_form: None,
                },
            );
        }

        let outer_skip: std::collections::HashSet<&String> = calibration
            .filters
            .method_ref_outer_skip
            .iter()
            .collect();
        let inner_skip: std::collections::HashSet<&String> = calibration
            .filters
            .method_ref_inner_skip
            .iter()
            .collect();

        let mut method_refs_by_inner: indexmap::IndexMap<String, Vec<MethodMember>> =
            indexmap::IndexMap::new();
        let mut assoc_const_members: indexmap::IndexMap<String, Vec<String>> =
            indexmap::IndexMap::new();
        let mut variant_ref_members: indexmap::IndexMap<String, Vec<String>> =
            indexmap::IndexMap::new();
        for ent in &usages.ast_method_ref_usages {
            if ent.outer.is_empty() || ent.inner.is_empty() {
                continue;
            }
            if outer_skip.contains(&ent.outer) {
                continue;
            }
            if inner_skip.contains(&ent.inner) {
                continue;
            }
            let defn = type_def_lookup
                .get(&ent.outer)
                .or_else(|| trait_def_lookup.get(&ent.outer));
            let defn = match defn {
                Some(d) => d,
                None => continue,
            };
            // Resolution gate (R8 slice 2): a member whose outer
            // resolves away from the declaring crate is not part of
            // this workspace family.
            let using = resolve_crate_for_file(&ent.file, &crate_dirs);
            let origin = resolve_site_origin(
                &ent.file,
                &ent.outer,
                ent.qualifier.as_deref(),
                &using,
                &import_maps,
                &vocab,
                &local_decl_crates,
                &local_mod_crates,
            );
            if !site_credits(&origin, &using, &defn.crate_name, &ent.outer, &facades) {
                continue;
            }
            // Labels routing (R8 slice 3): constant-shaped inners are
            // associated-constant accesses, not method refs - divert
            // to globals:<O>::<CONST> synthesis; never family-fodder.
            if is_constant_shaped(&ent.inner) {
                assoc_const_members
                    .entry(format!("{}::{}", ent.outer, ent.inner))
                    .or_default()
                    .push(ent.file.clone());
                continue;
            }
            // Variants routing (R8 slice 3, method-ref stream): a
            // variant-shaped inner on an enum outer is a constructor
            // REFERENCE (iter.map(Value::String)) - the enum's
            // surface, not a method family. Divert into the
            // type_usage pool so translate collapses it into
            // structure:<O>; `_::<Variant>` families across unrelated
            // enums are not architectural families.
            if is_variant_shaped(&ent.inner) && enum_names.contains(&ent.outer) {
                variant_ref_members
                    .entry(format!("{}::{}", ent.outer, ent.inner))
                    .or_default()
                    .push(ent.file.clone());
                continue;
            }
            method_refs_by_inner
                .entry(ent.inner.clone())
                .or_default()
                .push(MethodMember {
                    outer: ent.outer.clone(),
                    defining_crate: defn.crate_name.clone(),
                    file: ent.file.clone(),
                });
        }

        for (key, files) in assoc_const_members {
            let outer = key.split("::").next().unwrap_or("");
            let defn = match type_def_lookup
                .get(outer)
                .or_else(|| trait_def_lookup.get(outer))
            {
                Some(d) => d,
                None => continue,
            };
            let pattern = format!("assoc_const:{}", key);
            if should_skip_pattern(&pattern, calibration) || metrics.contains_key(&pattern) {
                continue;
            }
            let defining_crate = defn.crate_name.clone();
            let (intra, inter, example_count, curated_count) =
                count_usages(&files, &defining_crate, &crate_dirs);
            let total = intra + inter;
            let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
            metrics.insert(
                pattern,
                PatternMetric {
                    defining_crate: Some(defining_crate),
                    intra_count: intra,
                    inter_count: inter,
                    inter_ratio: round3(ratio),
                    is_pub: true,
                    example_count: serde_json::Value::from(example_count as f64),
                    curated_example_count: curated_count,
                    sub_form: None,
                },
            );
        }

        // Variant constructor references merge into the type_usage
        // pool (additional sites for an existing factory-call metric,
        // or a fresh one); translate's enum-variant collapse then
        // folds them into the structure:<O> aggregate.
        for (key, files) in variant_ref_members {
            let outer = key.split("::").next().unwrap_or("");
            let defn = match type_def_lookup.get(outer) {
                Some(d) => d,
                None => continue,
            };
            let pattern = format!("type_usage:{}", key);
            if should_skip_pattern(&pattern, calibration) {
                continue;
            }
            let defining_crate = defn.crate_name.clone();
            let (intra, inter, example_count, curated_count) =
                count_usages(&files, &defining_crate, &crate_dirs);
            let total = intra + inter;
            let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
            merge_metric(
                &mut metrics,
                pattern,
                PatternMetric {
                    defining_crate: Some(defining_crate),
                    intra_count: intra,
                    inter_count: inter,
                    inter_ratio: round3(ratio),
                    is_pub: defn.visibility.starts_with("pub"),
                    example_count: serde_json::Value::from(example_count as f64),
                    curated_example_count: curated_count,
                    sub_form: None,
                },
            );
        }

        for (inner, members) in method_refs_by_inner {
            let distinct_outers: indexmap::IndexSet<&String> =
                members.iter().map(|m| &m.outer).collect();
            if distinct_outers.len() < calibration.picker.family.method_ref_min_outers {
                continue;
            }
            let pattern = format!("method_ref:_::{}", inner);
            if should_skip_pattern(&pattern, calibration) {
                continue;
            }
            if metrics.contains_key(&pattern) {
                continue;
            }
            let mut counts: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
            for m in &members {
                *counts.entry(m.defining_crate.clone()).or_default() += 1;
            }
            let mut defining_crate = String::new();
            let mut max_count: usize = 0;
            for (k, v) in &counts {
                if defining_crate.is_empty() || *v > max_count {
                    defining_crate = k.clone();
                    max_count = *v;
                }
            }
            let files: Vec<String> = members.iter().map(|m| m.file.clone()).collect();
            let (intra, inter, example_count, curated_count) =
                count_usages(&files, &defining_crate, &crate_dirs);
            let total = intra + inter;
            let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
            metrics.insert(
                pattern,
                PatternMetric {
                    defining_crate: Some(defining_crate),
                    intra_count: intra,
                    inter_count: inter,
                    inter_ratio: round3(ratio),
                    is_pub: true,
                    example_count: serde_json::Value::from(example_count as f64),
                    curated_example_count: curated_count,
                    sub_form: None,
                },
            );
        }

        // Free-fn synthesis: the utilities group's FreeFn members.
        // Call heads resolve per site; only PUB fns declared in src
        // files of the resolved crate become candidates. A bare
        // unimported call is same-module scope, so the Unresolved
        // fallback credits the using crate's own declaration. Mod-
        // qualified heads (util::helper) stay with the items walker's
        // implementation_functions channel - only bare and
        // crate-member-qualified calls feed free_fn (no dual keys).
        let mut free_fn_decls: std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        > = std::collections::HashMap::new();
        for f in &all_facts.fns {
            if f.get("brace_depth").and_then(|v| v.as_u64()).unwrap_or(99) != 0 {
                continue;
            }
            let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
            if !is_src_file(file) {
                continue;
            }
            if let (Some(n), Some(c)) = (
                f.get("name").and_then(|v| v.as_str()),
                f.get("crate").and_then(|v| v.as_str()),
            ) {
                let vis = f.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
                free_fn_decls
                    .entry(n.to_string())
                    .or_default()
                    .entry(c.to_string())
                    .or_insert_with(|| vis.to_string());
            }
        }
        let mut free_fn_sites: indexmap::IndexMap<
            String,
            indexmap::IndexMap<String, Vec<String>>,
        > = indexmap::IndexMap::new();
        for ent in &usages.ast_fn_call_usages {
            if ent.name.is_empty() {
                continue;
            }
            let using = resolve_crate_for_file(&ent.file, &crate_dirs);
            if using.is_empty() {
                continue;
            }
            let candidate: Option<String> = match ent.qualifier.as_deref() {
                Some(q) => vocab.resolve_root(&using, q).cloned(),
                None => match resolve_ident_origin(
                    &ent.file,
                    &ent.name,
                    &using,
                    &import_maps,
                    &vocab,
                    &local_decl_crates,
                ) {
                    IdentOrigin::Workspace(c) => Some(c),
                    IdentOrigin::SelfCrate | IdentOrigin::Unresolved => Some(using.clone()),
                    IdentOrigin::Std | IdentOrigin::External => None,
                },
            };
            let Some(c) = candidate else { continue };
            let vis_ok = free_fn_decls
                .get(&ent.name)
                .and_then(|m| m.get(&c))
                .map(|v| v.starts_with("pub"))
                .unwrap_or(false);
            let target = if vis_ok {
                Some(c.clone())
            } else {
                // Facade redirect: the binding crate re-exports the
                // fn by name (unique pub workspace declaration wins)
                // or wholesale re-exports a namespace that declares
                // it (unique pub declaration inside the closure wins).
                free_fn_decls.get(&ent.name).and_then(|m| {
                    let pubs: Vec<&String> = m
                        .iter()
                        .filter(|(_, v)| v.starts_with("pub"))
                        .map(|(k, _)| k)
                        .collect();
                    let leaf = facades
                        .leaf_reexports
                        .get(&c)
                        .map(|s| s.contains(&ent.name))
                        .unwrap_or(false);
                    if leaf && pubs.len() == 1 {
                        return Some(pubs[0].clone());
                    }
                    let ns_pubs: Vec<&&String> = pubs
                        .iter()
                        .filter(|k| {
                            facades
                                .ns_closure
                                .get(&c)
                                .map(|s| s.contains(**k))
                                .unwrap_or(false)
                        })
                        .collect();
                    if ns_pubs.len() == 1 {
                        Some((*ns_pubs[0]).clone())
                    } else {
                        None
                    }
                })
            };
            let Some(target) = target else { continue };
            free_fn_sites
                .entry(ent.name.clone())
                .or_default()
                .entry(target)
                .or_default()
                .push(ent.file.clone());
        }
        for (name, by_crate) in free_fn_sites {
            let pattern = format!("free_fn:{}", name);
            if should_skip_pattern(&pattern, calibration) || metrics.contains_key(&pattern) {
                continue;
            }
            let mut best: Option<(String, Vec<String>)> = None;
            for (c, files) in by_crate {
                let replace = match &best {
                    None => true,
                    Some((_, bf)) => files.len() > bf.len(),
                };
                if replace {
                    best = Some((c, files));
                }
            }
            let Some((defining_crate, files)) = best else { continue };
            let (intra, inter, example_count, curated_count) =
                count_usages(&files, &defining_crate, &crate_dirs);
            let total = intra + inter;
            let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
            metrics.insert(
                pattern,
                PatternMetric {
                    defining_crate: Some(defining_crate),
                    intra_count: intra,
                    inter_count: inter,
                    inter_ratio: round3(ratio),
                    is_pub: true,
                    example_count: serde_json::Value::from(example_count as f64),
                    curated_example_count: curated_count,
                    sub_form: None,
                },
            );
        }
    }

    let grouped =
        translate_to_group_keys(metrics, &type_def_lookup, &trait_def_lookup, &enum_names);
    classify_sub_forms(grouped, all_facts, calibration)
}

/// What: translate the kind:name pattern_metrics keys to the picks-data
/// model's group:name shape per the refactor plan
/// (`notes/know_rust/tasks/picks-data-model-refactor.md`).
///
/// Why: phase R3 picker migration. The current emission uses syntactic
/// kinds; the refactored shape groups patterns by their semantic role
/// (traits / derives / structure / implementation_functions / utilities)
/// so the picker + emit phases consume the new model.
///
/// Where: called at the tail of `compute_pattern_metrics`. The mapping:
///
/// - `trait_impl:T` -> `traits:T` (the trait is the protagonist; impls
///   signal architectural usage)
/// - `derive:T` / `attr_macro:M` -> `configuring:T` (configured-via-
///   attributes derives + configuring attribute-macros; broadened from
///   the former derives-only group)
/// - `reg_macro:M` -> `utilities:M`
/// - `pub_type:X` -> `structure:X` (when X is a struct / enum / union /
///   type alias) or `traits:X` (when X is a trait def), discriminated
///   via the type_def_lookup vs trait_def_lookup probe.
/// - `method_ref:_::i` -> `implementation_functions:_::i`
/// - `type_usage:O::i` -> bridge: emits BOTH
///   `implementation_functions:O::i` (per-method metric copied verbatim)
///   AND `structure:O` (aggregated across all `type_usage:O::*` siblings)
///
/// Collisions (e.g. `traits:Plugin` reached from both `trait_impl:Plugin`
/// and `pub_type:Plugin`) sum-merge intra/inter/example counts; the
/// defining_crate is kept from the first contributor (impls-side, which
/// is computed first).
fn translate_to_group_keys(
    old: indexmap::IndexMap<String, PatternMetric>,
    type_def_lookup: &indexmap::IndexMap<String, PatternDef>,
    trait_def_lookup: &indexmap::IndexMap<String, PatternDef>,
    enum_names: &std::collections::HashSet<String>,
) -> indexmap::IndexMap<String, PatternMetric> {
    let mut new: indexmap::IndexMap<String, PatternMetric> = indexmap::IndexMap::new();
    let mut structure_aggregates: indexmap::IndexMap<String, Vec<PatternMetric>> =
        indexmap::IndexMap::new();

    for (key, metric) in old {
        let (kind, inner) = match key.split_once(':') {
            Some((k, n)) => (k.to_string(), n.to_string()),
            None => continue,
        };
        match kind.as_str() {
            "trait_impl" => {
                let new_key = format!("traits:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "derive" | "attr_macro" => {
                // Configuring: configured-via-attributes derives AND
                // configuring attribute-macros (`#[tokio::main]` et al.)
                // both wire a type into a framework via compile-time
                // codegen. Broadened from the former derives-only group.
                let new_key = format!("configuring:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "reg_macro" => {
                // Invocation-driven registration macros stay utilities
                // (they are call sites, not attribute-driven integration).
                let new_key = format!("utilities:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "free_fn" => {
                // Standalone fns are the utilities group's FreeFn
                // members (the picks-data model always reserved the
                // slot; the call-head capture fills it).
                let new_key = format!("utilities:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "pub_type" => {
                let group = if type_def_lookup.contains_key(&inner) {
                    "structure"
                } else if trait_def_lookup.contains_key(&inner) {
                    "traits"
                } else {
                    continue;
                };
                let new_key = format!("{}:{}", group, inner);
                merge_metric(&mut new, new_key, metric);
            }
            "method_ref" => {
                // inner shape is `_::method`; preserve verbatim.
                let new_key = format!("implementation_functions:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "assoc_const" => {
                // Labels (R8 slice 3): associated-constant accesses
                // are globals-group picks.
                let new_key = format!("globals:{}", inner);
                merge_metric(&mut new, new_key, metric);
            }
            "type_usage" => {
                // Bridge: emit BOTH structure:<outer> (aggregated below)
                // and implementation_functions:<outer>::<inner>. The
                // structure side requires the outer to be a workspace
                // TYPE def - the defining-crate lookup also resolves
                // mod / crate outers (env::args et al.), and a module
                // is not a structure pick. Enum-VARIANT inners collapse
                // into the structure aggregate only (R8 slice 3):
                // variants are the enum's surface, not per-variant
                // implementation_functions picks. A module/crate-outer
                // `::main` is a language ENTRY POINT, not consumable
                // API - no pair key (cosmic-epoch's pop-launcher
                // plugin mains); a TYPE-outer `main` assoc fn stays.
                let (outer, inner_part) = inner
                    .split_once("::")
                    .map(|(o, i)| (o, i))
                    .unwrap_or((inner.as_str(), ""));
                let collapses = enum_names.contains(outer) && is_variant_shaped(inner_part);
                let entry_main =
                    inner_part == "main" && !type_def_lookup.contains_key(outer);
                if !collapses && !entry_main {
                    let impl_fn_key = format!("implementation_functions:{}", inner);
                    merge_metric(&mut new, impl_fn_key, metric.clone());
                }
                if type_def_lookup.contains_key(outer) {
                    structure_aggregates
                        .entry(outer.to_string())
                        .or_default()
                        .push(metric);
                }
            }
            _ => {}
        }
    }

    // Aggregate structure:<outer> across all type_usage siblings.
    for (outer, parts) in structure_aggregates {
        if parts.is_empty() {
            continue;
        }
        let intra: usize = parts.iter().map(|m| m.intra_count).sum();
        let inter: usize = parts.iter().map(|m| m.inter_count).sum();
        let total = intra + inter;
        let ratio = if total > 0 { inter as f64 / total as f64 } else { 0.0 };
        let example_count: f64 = parts
            .iter()
            .map(|m| m.example_count.as_f64().unwrap_or(0.0))
            .sum();
        let curated: usize = parts.iter().map(|m| m.curated_example_count).sum();
        let is_pub = parts.iter().any(|m| m.is_pub);
        let defining_crate = parts
            .iter()
            .find_map(|m| m.defining_crate.clone());
        let aggregated = PatternMetric {
            defining_crate,
            intra_count: intra,
            inter_count: inter,
            inter_ratio: round3(ratio),
            is_pub,
            example_count: serde_json::Value::from((example_count * 100.0).round() / 100.0),
            curated_example_count: curated,
            sub_form: None,
        };
        let key = format!("structure:{}", outer);
        merge_metric(&mut new, key, aggregated);
    }

    new
}

/// What: insert a PatternMetric under `key`, or sum-merge into an
/// existing entry when present.
///
/// Why: the kind -> group mapping can route multiple kind:name sources
/// into the same group:name key (e.g. trait_impl:Plugin and
/// pub_type:Plugin both target traits:Plugin); summing their counts
/// preserves both signals' contributions to the architectural score.
fn merge_metric(
    map: &mut indexmap::IndexMap<String, PatternMetric>,
    key: String,
    m: PatternMetric,
) {
    if let Some(existing) = map.get_mut(&key) {
        existing.intra_count += m.intra_count;
        existing.inter_count += m.inter_count;
        let total = existing.intra_count + existing.inter_count;
        existing.inter_ratio = if total > 0 {
            round3(existing.inter_count as f64 / total as f64)
        } else {
            0.0
        };
        existing.is_pub |= m.is_pub;
        let existing_ec = existing.example_count.as_f64().unwrap_or(0.0);
        let new_ec = m.example_count.as_f64().unwrap_or(0.0);
        existing.example_count =
            serde_json::Value::from(((existing_ec + new_ec) * 100.0).round() / 100.0);
        existing.curated_example_count += m.curated_example_count;
        if existing.defining_crate.is_none() {
            existing.defining_crate = m.defining_crate;
        }
    } else {
        map.insert(key, m);
    }
}

struct MethodMember {
    outer: String,
    defining_crate: String,
    file: String,
}

/// What: true for identifiers shaped like associated CONSTANTS - no
/// lowercase characters (`ZERO`, `Y`, `IDENTITY`, `X1`).
///
/// Why: constant accesses are LABELS in the what-why-where taxonomy
/// (the_user: "those look exactly like labels") and route to the
/// globals group, not implementation_functions; they also never form
/// method_ref `_::` families. A general shape signal, not a name
/// list.
fn is_constant_shaped(inner: &str) -> bool {
    !inner.is_empty() && !inner.chars().any(|c| c.is_lowercase())
}

/// What: true for enum-VARIANT-shaped inners: upper-initial with at
/// least one lowercase character (`Text`, `Io`, `Rgb`).
fn is_variant_shaped(inner: &str) -> bool {
    inner.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        && inner.chars().any(|c| c.is_lowercase())
}

fn count_usages(
    files: &[String],
    defining_crate: &str,
    crate_dirs: &indexmap::IndexMap<String, String>,
) -> (usize, usize, usize, usize) {
    let mut intra = 0usize;
    let mut inter = 0usize;
    let mut example_files: indexmap::IndexSet<String> = indexmap::IndexSet::new();
    let mut curated_files: indexmap::IndexSet<String> = indexmap::IndexSet::new();
    for file_path in files {
        let using = resolve_crate_for_file(file_path, crate_dirs);
        if using.is_empty() {
            continue;
        }
        // Evidence-dir sites are evidence, not usage (the example-
        // evidence alignment, uniform with the main counting loop;
        // tests/ + benches/ never reach these streams - excluded
        // from the walk - and stay evidence-only defensively).
        if file_path.contains("/examples/") || file_path.starts_with("examples/") {
            example_files.insert(file_path.clone());
            curated_files.insert(file_path.clone());
            continue;
        } else if file_path.contains("/tests/") || file_path.starts_with("tests/") {
            example_files.insert(file_path.clone());
            continue;
        } else if file_path.contains("/benches/") || file_path.starts_with("benches/") {
            example_files.insert(file_path.clone());
            continue;
        }
        if using == defining_crate {
            intra += 1;
        } else {
            inter += 1;
        }
    }
    (intra, inter, example_files.len(), curated_files.len())
}

/// What: resolve which workspace crate owns a given file path via
/// longest-prefix match against each crate's directory.
///
/// Why: ast usage entries record the workspace-relative file path
/// where each identifier was seen; mapping back to the using crate
/// is the join that drives intra/inter counts for synthesized
/// pub_type + method_ref pattern_metrics entries.
///
/// Where: called from `compute_pattern_metrics` for pub_type +
/// method_ref synthesis, and from `crate::characterize::run::characterize`
/// for the `ast_type_refs` + `ast_method_refs` facts population.
pub(crate) fn resolve_crate_for_file(
    file_path: &str,
    crate_dirs: &indexmap::IndexMap<String, String>,
) -> String {
    let norm = file_path.replace('\\', "/");
    let mut best = String::new();
    let mut best_len: i32 = -1;
    for (name, dir_str) in crate_dirs {
        let d = dir_str.trim_end_matches('/');
        if d.is_empty() || d == "." {
            if best_len < 0 {
                best = name.clone();
                best_len = 0;
            }
            continue;
        }
        let prefix = format!("{}/", d);
        if norm.starts_with(&prefix) && (prefix.len() as i32) > best_len {
            best = name.clone();
            best_len = prefix.len() as i32;
        }
    }
    best
}

struct PatternDef {
    crate_name: String,
    visibility: String,
    macro_exported: bool,
}

fn build_impl_target_count(
    impls: &[serde_json::Value],
) -> std::collections::HashMap<String, std::collections::HashMap<String, usize>> {
    let mut counts: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
        std::collections::HashMap::new();
    for i in impls {
        let type_name = i.get("type").and_then(|v| v.as_str());
        let crate_name = i.get("crate").and_then(|v| v.as_str());
        if let (Some(t), Some(c)) = (type_name, crate_name) {
            *counts
                .entry(t.to_string())
                .or_default()
                .entry(c.to_string())
                .or_insert(0) += 1;
        }
    }
    counts
}

fn build_type_lookup(
    types: &[serde_json::Value],
    impl_target_count: &std::collections::HashMap<String, std::collections::HashMap<String, usize>>,
) -> (
    indexmap::IndexMap<String, PatternDef>,
    std::collections::HashMap<(String, String), String>,
) {
    let mut candidates: indexmap::IndexMap<String, Vec<String>> = indexmap::IndexMap::new();
    let mut visibilities: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for t in types {
        let name = match t.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let crate_name = match t.get("crate").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => continue,
        };
        let entry = candidates.entry(name.to_string()).or_default();
        if !entry.contains(&crate_name.to_string()) {
            entry.push(crate_name.to_string());
        }
        let vis = t.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
        visibilities.insert((name.to_string(), crate_name.to_string()), vis.to_string());
    }
    let mut lookup: indexmap::IndexMap<String, PatternDef> = indexmap::IndexMap::new();
    for (name, cands) in candidates {
        let canonical = if cands.len() <= 1 {
            cands.into_iter().next()
        } else {
            let counts = impl_target_count.get(&name);
            let mut best: Option<String> = None;
            let mut best_count: usize = 0;
            for c in cands.into_iter() {
                let cnt = counts.and_then(|m| m.get(&c)).copied().unwrap_or(0);
                if best.is_none() || cnt > best_count {
                    best = Some(c);
                    best_count = cnt;
                }
            }
            best
        };
        if let Some(canonical) = canonical {
            let vis = visibilities
                .get(&(name.clone(), canonical.clone()))
                .cloned()
                .unwrap_or_default();
            lookup.insert(
                name,
                PatternDef {
                    crate_name: canonical,
                    visibility: vis,
                    macro_exported: false,
                },
            );
        }
    }
    (lookup, visibilities)
}

fn build_trait_lookup(traits: &[serde_json::Value]) -> indexmap::IndexMap<String, PatternDef> {
    let mut lookup: indexmap::IndexMap<String, PatternDef> = indexmap::IndexMap::new();
    for t in traits {
        let name = match t.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let crate_name = match t.get("crate").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => continue,
        };
        if !lookup.contains_key(name) {
            let vis = t.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
            lookup.insert(
                name.to_string(),
                PatternDef {
                    crate_name: crate_name.to_string(),
                    visibility: vis.to_string(),
                    macro_exported: false,
                },
            );
        }
    }
    lookup
}

fn build_macro_def_lookup(
    macro_defs: &[serde_json::Value],
) -> indexmap::IndexMap<String, PatternDef> {
    let mut lookup: indexmap::IndexMap<String, PatternDef> = indexmap::IndexMap::new();
    for m in macro_defs {
        let name = match m.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let crate_name = match m.get("crate").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => continue,
        };
        if !lookup.contains_key(name) {
            let vis = m.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
            let exported = m
                .get("macro_exported")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            lookup.insert(
                name.to_string(),
                PatternDef {
                    crate_name: crate_name.to_string(),
                    visibility: vis.to_string(),
                    macro_exported: exported,
                },
            );
        }
    }
    lookup
}

fn build_mod_lookup(mods: &[serde_json::Value]) -> indexmap::IndexMap<String, PatternDef> {
    let mut lookup: indexmap::IndexMap<String, PatternDef> = indexmap::IndexMap::new();
    for m in mods {
        let name = match m.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let crate_name = match m.get("crate").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => continue,
        };
        if !lookup.contains_key(name) {
            let vis = m.get("visibility").and_then(|v| v.as_str()).unwrap_or("");
            lookup.insert(
                name.to_string(),
                PatternDef {
                    crate_name: crate_name.to_string(),
                    visibility: vis.to_string(),
                    macro_exported: false,
                },
            );
        }
    }
    lookup
}

fn build_crate_name_lookup(
    all_facts: &WorkspaceFacts,
) -> indexmap::IndexMap<String, PatternDef> {
    let mut crate_names_seen: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let lists: &[&[serde_json::Value]] = &[
        &all_facts.impls,
        &all_facts.derives,
        &all_facts.uses,
        &all_facts.types,
        &all_facts.traits,
        &all_facts.fns,
        &all_facts.macros,
        &all_facts.macro_defs,
        &all_facts.type_usages,
        &all_facts.mods,
        &all_facts.example_type_usages,
    ];
    for list in lists {
        for f in *list {
            if let Some(c) = f.get("crate").and_then(|v| v.as_str()) {
                crate_names_seen.insert(c.to_string());
            }
        }
    }
    let mut lookup: indexmap::IndexMap<String, PatternDef> = indexmap::IndexMap::new();
    for cn in crate_names_seen {
        lookup.insert(
            cn.clone(),
            PatternDef {
                crate_name: cn,
                visibility: "pub".to_string(),
                macro_exported: false,
            },
        );
    }
    lookup
}

/// What: which lookup chain produced a pattern's defining-crate
/// attribution. Mod / crate-namespace fallbacks are weaker evidence
/// than a type / trait / macro declaration and get stricter
/// zero-credit handling in the counting loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefSource {
    TypeDef,
    TraitDef,
    MacroDef,
    Mod,
    CrateName,
}

fn pattern_def<'a>(
    kind: &str,
    pattern_inner: &str,
    type_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    trait_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    macro_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    mod_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    crate_name_lookup: &'a indexmap::IndexMap<String, PatternDef>,
) -> Option<(&'a PatternDef, DefSource)> {
    match kind {
        "trait_impl" | "derive" => trait_def_lookup
            .get(pattern_inner)
            .map(|d| (d, DefSource::TraitDef)),
        "type_usage" => {
            let outer = pattern_inner.split("::").next().unwrap_or("");
            type_def_lookup
                .get(outer)
                .map(|d| (d, DefSource::TypeDef))
                .or_else(|| mod_def_lookup.get(outer).map(|d| (d, DefSource::Mod)))
                .or_else(|| {
                    crate_name_lookup
                        .get(outer)
                        .map(|d| (d, DefSource::CrateName))
                })
        }
        "reg_macro" | "attr_macro" => macro_def_lookup
            .get(pattern_inner)
            .map(|d| (d, DefSource::MacroDef)),
        _ => None,
    }
}

fn pattern_match(kind: &str, pattern_inner: &str, fact: &serde_json::Value) -> bool {
    match kind {
        "trait_impl" => {
            fact.get("trait").and_then(|v| v.as_str()) == Some(pattern_inner)
                && !fact.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false)
        }
        "derive" => fact.get("trait").and_then(|v| v.as_str()) == Some(pattern_inner),
        "type_usage" => fact.get("name").and_then(|v| v.as_str()) == Some(pattern_inner),
        "reg_macro" => {
            fact.get("kind").and_then(|v| v.as_str()) == Some("macro_invocation")
                && fact.get("name").and_then(|v| v.as_str()) == Some(pattern_inner)
        }
        "attr_macro" => {
            fact.get("kind").and_then(|v| v.as_str()) == Some("attr_macro")
                && fact.get("name").and_then(|v| v.as_str()) == Some(pattern_inner)
        }
        _ => false,
    }
}

fn compute_example_counts(
    example_type_usages: &[serde_json::Value],
    test_weight: f64,
    bench_weight: f64,
) -> (
    indexmap::IndexMap<String, f64>,
    indexmap::IndexMap<String, usize>,
) {
    let mut by_name: indexmap::IndexMap<String, [std::collections::BTreeSet<String>; 3]> =
        indexmap::IndexMap::new();
    for tu in example_type_usages {
        let nm = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let f = tu.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if nm.is_empty() || f.is_empty() {
            continue;
        }
        let cats = by_name
            .entry(nm.to_string())
            .or_insert_with(Default::default);
        if f.contains("/examples/") || f.starts_with("examples/") {
            cats[0].insert(f.to_string());
        } else if f.contains("/tests/") || f.starts_with("tests/") {
            cats[1].insert(f.to_string());
        } else if f.contains("/benches/") || f.starts_with("benches/") {
            cats[2].insert(f.to_string());
        }
    }
    let mut weighted: indexmap::IndexMap<String, f64> = indexmap::IndexMap::new();
    let mut curated: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    for (nm, cats) in by_name {
        let w = cats[0].len() as f64
            + cats[1].len() as f64 * test_weight
            + cats[2].len() as f64 * bench_weight;
        weighted.insert(nm.clone(), w);
        curated.insert(nm, cats[0].len());
    }
    (weighted, curated)
}

/// What: count distinct example-dir files among the facts matching a
/// (kind, inner) pattern. Returns 0 for type_usage, which keeps its
/// example_type_usages partition as the evidence source.
///
/// Why: the broad-channels design rule (the_user 2026-06-09;
/// notes/know_rust/working/02_picks_data.md "Design rule: broad
/// channels, mechanically filled"): example evidence is "any pattern
/// occurrence in a curated example". trait_impl / derive / reg_macro /
/// attr_macro facts from examples/ files were recorded as plain
/// occurrences with no evidence credit - a capture gap, not a design
/// boundary. Weight stays 1x per the 0.0.13k decision; tests/ +
/// benches/ never reach these streams (excluded from the walk since
/// 0.0.25), so curated == weighted for these kinds.
///
/// Where: called from compute_pattern_metrics' seen_patterns loop for
/// both the defn-present and defn-absent branches.
fn example_file_count(kind: &str, inner: &str, source: &[serde_json::Value]) -> usize {
    if kind == "type_usage" {
        return 0;
    }
    let mut files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for fact in source {
        if !pattern_match(kind, inner, fact) {
            continue;
        }
        if let Some(f) = fact.get("file").and_then(|v| v.as_str()) {
            let norm = f.replace('\\', "/");
            if norm.contains("/examples/") || norm.starts_with("examples/") {
                files.insert(norm);
            }
        }
    }
    files.len()
}

fn example_count_value(
    kind: &str,
    inner: &str,
    weighted: &indexmap::IndexMap<String, f64>,
) -> serde_json::Value {
    if kind != "type_usage" {
        return serde_json::Value::from(0);
    }
    let v = weighted.get(inner).copied().unwrap_or(0.0);
    let rounded = (v * 100.0).round() / 100.0;
    serde_json::Value::from(rounded)
}

fn curated_example_count(
    kind: &str,
    inner: &str,
    curated: &indexmap::IndexMap<String, usize>,
) -> usize {
    if kind != "type_usage" {
        return 0;
    }
    curated.get(inner).copied().unwrap_or(0)
}

fn round3(x: f64) -> f64 {
    format!("{:.3}", x).parse().unwrap_or(x)
}

/// What: return `true` when the pattern key (`kind:name` pre-
/// translation) matches any substring in
/// `calibration.filters.pattern_skip_substrings`. Drops the
/// pattern before metric construction.
///
/// Why: NF5 - suppresses nushell's `Value::test_*` family + similar
/// test-helper noise from the picker pool. The substring match
/// (rather than prefix) catches both outer-anchored shapes like
/// `type_usage:Value::test_string` and inner-anchored shapes like
/// `method_ref:_::test_helper`.
///
/// Where: called at the three pattern-insertion sites in
/// `compute_pattern_metrics` (the main seen_patterns loop + the
/// pub_type synth loop + the method_ref synth loop), and by
/// `decl_api::decl_api_channel` before minting declaration keys (a
/// minted key must not resurrect a family NF5 dropped from the
/// usage streams).
pub(crate) fn should_skip_pattern(pattern: &str, calibration: &Calibration) -> bool {
    calibration
        .filters
        .pattern_skip_substrings
        .iter()
        .any(|sub| pattern.contains(sub.as_str()))
}

/// What: walk the group-keyed pattern_metrics and populate `sub_form`
/// for the groups a GENERAL structural signal discriminates.
/// `structure:` patterns get `Foundational` / `Incidental` (by
/// intra+inter+example count); `traits:` get `Lifecycle` / `Marker`
/// (by workspace impl-count); `utilities:` get `Macro` (the picker
/// currently only emits macros into utilities). `configuring:`,
/// `implementation_functions:`, `trait_functions:`, `globals:` keep
/// `None` - no general signal backs a mechanical sub-classification
/// (mechanical-broad, subagent-fine per
/// `notes/know_rust/working/05_calibration.md`).
///
/// Why: the prose-budget matrix looks up `(group, sub_form, set)`; the
/// classifier output is the matrix's row selector. Only sub-forms a
/// general signal supports are retained - the removed derive
/// configured-vs-marker classifier leaned on a per-subject allowlist
/// (a cheat that does not generalize to unseen subjects), so the
/// `configuring` group is left to the subagent thought-experiment.
///
/// Where: called at the tail of `compute_pattern_metrics` after
/// `translate_to_group_keys`. Reads from `all_facts` (impls /
/// derives / attrs / macros) for the per-classifier signals.
fn classify_sub_forms(
    mut metrics: indexmap::IndexMap<String, PatternMetric>,
    all_facts: &WorkspaceFacts,
    calibration: &Calibration,
) -> indexmap::IndexMap<String, PatternMetric> {
    for (key, metric) in metrics.iter_mut() {
        let (group, name) = match key.split_once(':') {
            Some((g, n)) => (g, n),
            None => continue,
        };
        metric.sub_form = match group {
            "structure" => Some(classify_structure(metric, calibration)),
            "traits" => Some(classify_traits(name, &all_facts.impls, calibration)),
            "utilities" => Some(classify_utilities(name, &all_facts.macros, &all_facts.fns)),
            // `configuring` carries no mechanical sub-form (mechanical-
            // broad, subagent-fine): the subagent thought-experiment does
            // the fine subclassification. So do impl/trait fns + globals.
            _ => None,
        };
    }
    metrics
}

/// What: classify a `structure:` pattern as `Foundational` (high
/// intra+inter+example architectural footprint) or `Incidental`
/// (low footprint; self-explanatory shape).
///
/// Why: foundational structures (bevy Transform / Res, ratatui
/// Buffer / Layout / Frame, nushell Value / PipelineData) earn the
/// most prose budget in the matrix because the reader cannot
/// reconstruct their non-inferrable semantics from name + signature
/// alone. Incidental structures (transient utility shapes,
/// crate-private state) get a thin budget.
///
/// Where: called per `structure:` pattern from `classify_sub_forms`.
/// Threshold comes from
/// `calibration.picker.classifier.foundational_min_total` (default 30
/// tuned on bevy's S5.1 structure picks); the_user can adjust via
/// calibration.toml without recompile.
fn classify_structure(metric: &PatternMetric, calibration: &Calibration) -> SubForm {
    let example = metric.example_count.as_f64().unwrap_or(0.0) as usize;
    let total = metric.intra_count + metric.inter_count + example;
    if total >= calibration.picker.classifier.foundational_min_total {
        SubForm::Foundational
    } else {
        SubForm::Incidental
    }
}

/// What: classify a `traits:` pattern as `Lifecycle` (rich
/// behavioural interface with significant in-workspace impl-count) or
/// `Marker` (zero-method / marker bound trait with sparse impls).
///
/// Why: lifecycle traits (bevy Plugin / System / SystemSet, tokio
/// Future / Stream / AsyncRead, helix Command) carry rich
/// architectural contracts; marker traits (Send / Sync / Sized,
/// auto-derived ecosystem markers) carry name-only semantics.
///
/// Mechanical heuristic (a GENERAL signal - no per-subject name-list):
/// count non-cfg-gated workspace impls of the trait; >=
/// `lifecycle_impl_threshold` (default 5) -> Lifecycle, else Marker.
/// The former `lifecycle_traits` override allowlist was removed as a
/// cheat (mechanical-broad, subagent-fine per
/// `notes/know_rust/working/05_calibration.md`).
///
/// Where: called per `traits:` pattern from `classify_sub_forms`.
fn classify_traits(
    trait_name: &str,
    impls: &[serde_json::Value],
    calibration: &Calibration,
) -> SubForm {
    let count = impls
        .iter()
        .filter(|i| {
            i.get("trait").and_then(|v| v.as_str()) == Some(trait_name)
                && !i.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false)
        })
        .count();
    if count >= calibration.picker.classifier.lifecycle_impl_threshold {
        SubForm::Lifecycle
    } else {
        SubForm::Marker
    }
}

/// What: classify a `utilities:` pattern as `Macro` (macro
/// invocation or attribute macro) or `FreeFn` (standalone
/// function).
///
/// Why: the picker currently emits utilities only from macro
/// facts (reg_macro / attr_macro -> utilities); the FreeFn branch
/// is structural completeness for future utilities sources.
///
/// Where: called per `utilities:` pattern from `classify_sub_forms`.
fn classify_utilities(
    name: &str,
    macros: &[serde_json::Value],
    _fns: &[serde_json::Value],
) -> SubForm {
    let any_macro = macros
        .iter()
        .any(|m| m.get("name").and_then(|v| v.as_str()) == Some(name));
    if any_macro {
        SubForm::Macro
    } else {
        SubForm::FreeFn
    }
}

