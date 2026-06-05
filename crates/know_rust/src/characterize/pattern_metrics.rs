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

    let test_weight = calibration.picker.example.test_weight;
    let bench_weight = calibration.picker.example.bench_weight;
    let (example_files_by_name, curated_example_count_by_name) =
        compute_example_counts(&all_facts.example_type_usages, test_weight, bench_weight);

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
                    example_count: example_count_value(kind, inner, &example_files_by_name),
                    curated_example_count: curated_example_count(kind, inner, &curated_example_count_by_name),
                },
            );
            continue;
        }
        let defn = defn.unwrap();
        let defining_crate = defn.crate_name.clone();
        let mut is_pub = defn.visibility.starts_with("pub");
        if !is_pub && defn.macro_exported {
            is_pub = true;
        }
        let mut intra = 0usize;
        let mut inter = 0usize;
        let source = match sources_by_kind.get(kind.as_str()) {
            Some(s) => *s,
            None => &[][..],
        };
        for fact in source {
            if !pattern_match(kind, inner, fact) {
                continue;
            }
            let using = match fact.get("crate").and_then(|v| v.as_str()) {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };
            if using == defining_crate {
                intra += 1;
            } else {
                inter += 1;
            }
        }
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
                example_count: example_count_value(kind, inner, &example_files_by_name),
                curated_example_count: curated_example_count(kind, inner, &curated_example_count_by_name),
            },
        );
    }

    let _ = type_visibility;

    if let Some(usages) = ast_usages {
        let crate_dirs: indexmap::IndexMap<String, String> = crates
            .iter()
            .map(|(k, v)| (k.clone(), v.dir.clone()))
            .collect();

        let mut ast_by_ident: indexmap::IndexMap<String, Vec<String>> = indexmap::IndexMap::new();
        for e in &usages.ast_fn_sig_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push(e.file.clone());
            }
        }
        for e in &usages.ast_field_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push(e.file.clone());
            }
        }
        for e in &usages.ast_type_alias_usages {
            if !e.ident.is_empty() {
                ast_by_ident
                    .entry(e.ident.clone())
                    .or_default()
                    .push(e.file.clone());
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
            if metrics.contains_key(&pattern) {
                continue;
            }
            let defining_crate = defn.crate_name.clone();
            let (intra, inter, example_count, curated_count) =
                count_usages(&hits, &defining_crate, &crate_dirs);
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
            method_refs_by_inner
                .entry(ent.inner.clone())
                .or_default()
                .push(MethodMember {
                    outer: ent.outer.clone(),
                    defining_crate: defn.crate_name.clone(),
                    file: ent.file.clone(),
                });
        }

        for (inner, members) in method_refs_by_inner {
            let distinct_outers: indexmap::IndexSet<&String> =
                members.iter().map(|m| &m.outer).collect();
            if distinct_outers.len() < calibration.picker.family.method_ref_min_outers {
                continue;
            }
            let pattern = format!("method_ref:_::{}", inner);
            if metrics.contains_key(&pattern) {
                continue;
            }
            let mut counts: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
            for m in &members {
                *counts.entry(m.defining_crate.clone()).or_default() += 1;
            }
            let defining_crate = counts
                .iter()
                .max_by_key(|(_, c)| **c)
                .map(|(k, _)| k.clone())
                .unwrap_or_default();
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
                },
            );
        }
    }

    metrics
}

struct MethodMember {
    outer: String,
    defining_crate: String,
    file: String,
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
        if using == defining_crate {
            intra += 1;
        } else {
            inter += 1;
        }
        if file_path.contains("/examples/") || file_path.starts_with("examples/") {
            example_files.insert(file_path.clone());
            curated_files.insert(file_path.clone());
        } else if file_path.contains("/tests/") || file_path.starts_with("tests/") {
            example_files.insert(file_path.clone());
        } else if file_path.contains("/benches/") || file_path.starts_with("benches/") {
            example_files.insert(file_path.clone());
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
            cands.into_iter().max_by_key(|c| {
                counts.and_then(|m| m.get(c)).copied().unwrap_or(0)
            })
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

fn pattern_def<'a>(
    kind: &str,
    pattern_inner: &str,
    type_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    trait_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    macro_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    mod_def_lookup: &'a indexmap::IndexMap<String, PatternDef>,
    crate_name_lookup: &'a indexmap::IndexMap<String, PatternDef>,
) -> Option<&'a PatternDef> {
    match kind {
        "trait_impl" | "derive" => trait_def_lookup.get(pattern_inner),
        "type_usage" => {
            let outer = pattern_inner.split("::").next().unwrap_or("");
            type_def_lookup
                .get(outer)
                .or_else(|| mod_def_lookup.get(outer))
                .or_else(|| crate_name_lookup.get(outer))
        }
        "reg_macro" | "attr_macro" => macro_def_lookup.get(pattern_inner),
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
    (x * 1000.0).round() / 1000.0
}
