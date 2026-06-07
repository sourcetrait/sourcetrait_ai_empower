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
        if should_skip_pattern(&pattern, calibration) {
            continue;
        }
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
                    sub_form: None,
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
            if should_skip_pattern(&pattern, calibration) {
                continue;
            }
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
    }

    let grouped = translate_to_group_keys(metrics, &type_def_lookup, &trait_def_lookup);
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
            "type_usage" => {
                // Bridge: emit BOTH structure:<outer> (aggregated below)
                // and implementation_functions:<outer>::<inner>.
                let outer = inner.split_once("::").map(|(o, _)| o).unwrap_or(&inner);
                let impl_fn_key = format!("implementation_functions:{}", inner);
                merge_metric(&mut new, impl_fn_key, metric.clone());
                structure_aggregates
                    .entry(outer.to_string())
                    .or_default()
                    .push(metric);
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
/// pub_type synth loop + the method_ref synth loop).
fn should_skip_pattern(pattern: &str, calibration: &Calibration) -> bool {
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

