use crate::*;

/// What: heuristic workspace-shape classifier mapping per-crate
/// dependency + pattern signals to one of the empirical shapes
/// (container / tight_framework / framework_with_users /
/// framework_product / submodule_aggregator / mixed / monolith /
/// empty).
///
/// Why: characterize.py's `classify_workspace_shape` (0.0.8 patch
/// 8e). The shape label drives emit's section routing: container
/// uses emit_container_routing, others use the standard
/// orientation. Per the_user 2026-06-03: 'we should not need to
/// alter any source-code. the point of the heuristics is to analyze
/// correctly, without intervention'.
///
/// Where: called from `crate::characterize::run::characterize` after
/// the partial fingerprint (per_crate + pattern_histogram) is
/// assembled.
pub fn classify_workspace_shape(
    per_crate: &indexmap::IndexMap<String, PerCrateFingerprint>,
    all_facts: &WorkspaceFacts,
) -> WorkspaceShape {
    let n_crates = per_crate.len();
    if n_crates == 0 {
        return WorkspaceShape {
            shape: "empty".to_string(),
            signals: indexmap::IndexMap::new(),
            reasoning: "no crates found in the workspace".to_string(),
        };
    }
    if n_crates == 1 {
        let mut signals: indexmap::IndexMap<String, serde_json::Value> =
            indexmap::IndexMap::new();
        signals.insert("n_crates".to_string(), serde_json::Value::from(1));
        return WorkspaceShape {
            shape: "monolith".to_string(),
            signals,
            reasoning: "single-crate workspace".to_string(),
        };
    }

    let signals = compute_shape_signals(per_crate, all_facts);
    let central_kind = signals
        .get("central_kind")
        .and_then(|v| v.as_str())
        .map(String::from);
    let uniqueness = signals
        .get("uniqueness_ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let dispersion = signals
        .get("kind_dominance_dispersion")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let leaf_ratio = signals
        .get("leaf_ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let hub_centrality = signals
        .get("hub_centrality")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let is_type_usage = central_kind.as_deref() == Some("type_usage");
    let is_neither = central_kind.is_none();

    if !is_type_usage && !is_neither && uniqueness > 0.7 && dispersion > 0.3 {
        let ck = central_kind.clone().unwrap_or_default();
        return WorkspaceShape {
            shape: "container".to_string(),
            signals,
            reasoning: format!(
                "central crate's dominant kind is `{}` (not the architectural type_usage axis) - the hub is shared infrastructure, not a framework; per-crate top kinds are diverse (dispersion {:.3}) indicating each member is its own topical library; patterns are mostly crate-isolated (uniqueness {:.3}). Each member should be probed individually for its architectural pattern.",
                ck, dispersion, uniqueness
            ),
        };
    }
    if dispersion < 0.1 && !is_type_usage && !is_neither {
        let ck = central_kind.clone().unwrap_or_default();
        return WorkspaceShape {
            shape: "submodule_aggregator".to_string(),
            signals,
            reasoning: format!(
                "per-crate top kinds align (dispersion {:.3}); central crate is `{}`-dominant rather than type_usage; structural shape suggests a config / versioning aggregator with many parallel topical submodules.",
                dispersion, ck
            ),
        };
    }
    if is_type_usage && uniqueness < 0.2 && dispersion < 0.1 {
        return WorkspaceShape {
            shape: "tight_framework".to_string(),
            signals,
            reasoning: format!(
                "central type_usage framework crate; patterns share heavily across crates (uniqueness {:.3}); crates align on the same dominant kind (dispersion {:.3}). Trace the central crate first.",
                uniqueness, dispersion
            ),
        };
    }
    if is_type_usage && leaf_ratio > 0.5 && hub_centrality > 0.7 {
        return WorkspaceShape {
            shape: "framework_with_users".to_string(),
            signals,
            reasoning: format!(
                "central type_usage framework crate; many leaf crates (leaf_ratio {:.3}); high hub centrality ({:.3}) indicates a framework + many independent client crates.",
                leaf_ratio, hub_centrality
            ),
        };
    }
    if is_type_usage {
        return WorkspaceShape {
            shape: "framework_product".to_string(),
            signals,
            reasoning: format!(
                "central type_usage framework crate; moderate pattern sharing (uniqueness {:.3}); typical layered product workspace shape.",
                uniqueness
            ),
        };
    }
    let ck = central_kind.unwrap_or_else(|| "None".to_string());
    WorkspaceShape {
        shape: "mixed".to_string(),
        signals,
        reasoning: format!(
            "signals don't match a known shape: central_kind={}, uniqueness={:.3}, dispersion={:.3}, leaf_ratio={:.3}, hub_centrality={:.3}. Emit as standard orientation; the shape may surface during follow-up analysis.",
            ck, uniqueness, dispersion, leaf_ratio, hub_centrality
        ),
    }
}

fn compute_shape_signals(
    per_crate: &indexmap::IndexMap<String, PerCrateFingerprint>,
    all_facts: &WorkspaceFacts,
) -> indexmap::IndexMap<String, serde_json::Value> {
    let crate_names: Vec<String> = {
        let mut v: Vec<String> = per_crate.keys().cloned().collect();
        v.sort();
        v
    };
    let n_crates = crate_names.len().max(1);

    let mut pattern_to_crates: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for it in &all_facts.impls {
        let trait_name = it.get("trait").and_then(|v| v.as_str());
        let cfg_gated = it.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false);
        let crate_name = it.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(t) = trait_name {
            if !cfg_gated {
                pattern_to_crates
                    .entry(format!("trait_impl:{}", t))
                    .or_default()
                    .insert(crate_name.to_string());
            }
        }
    }
    for d in &all_facts.derives {
        let trait_name = d.get("trait").and_then(|v| v.as_str());
        let crate_name = d.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(t) = trait_name {
            pattern_to_crates
                .entry(format!("derive:{}", t))
                .or_default()
                .insert(crate_name.to_string());
        }
    }
    for m in &all_facts.macros {
        let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let crate_name = m.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        if kind == "attr_macro" {
            pattern_to_crates
                .entry(format!("attr_macro:{}", name))
                .or_default()
                .insert(crate_name.to_string());
        } else if kind == "macro_invocation" {
            pattern_to_crates
                .entry(format!("reg_macro:{}", name))
                .or_default()
                .insert(crate_name.to_string());
        }
    }
    for tu in &all_facts.type_usages {
        let name = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let crate_name = tu.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if !name.is_empty() {
            pattern_to_crates
                .entry(format!("type_usage:{}", name))
                .or_default()
                .insert(crate_name.to_string());
        }
    }
    let unique_patterns = pattern_to_crates.len();
    let single_crate_patterns = pattern_to_crates
        .values()
        .filter(|s| s.len() == 1)
        .count();
    let uniqueness_ratio = if unique_patterns > 0 {
        single_crate_patterns as f64 / unique_patterns as f64
    } else {
        0.0
    };

    let mut per_crate_kinds: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
        std::collections::HashMap::new();
    for it in &all_facts.impls {
        let cfg_gated = it.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false);
        let crate_name = it.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if it.get("trait").and_then(|v| v.as_str()).is_some() && !cfg_gated {
            *per_crate_kinds
                .entry(crate_name.to_string())
                .or_default()
                .entry("trait_impl".to_string())
                .or_default() += 1;
        }
    }
    for d in &all_facts.derives {
        let crate_name = d.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        *per_crate_kinds
            .entry(crate_name.to_string())
            .or_default()
            .entry("derive".to_string())
            .or_default() += 1;
    }
    for m in &all_facts.macros {
        let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let crate_name = m.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "attr_macro" {
            *per_crate_kinds
                .entry(crate_name.to_string())
                .or_default()
                .entry("attr_macro".to_string())
                .or_default() += 1;
        } else if kind == "macro_invocation" {
            *per_crate_kinds
                .entry(crate_name.to_string())
                .or_default()
                .entry("reg_macro".to_string())
                .or_default() += 1;
        }
    }
    for tu in &all_facts.type_usages {
        let crate_name = tu.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        *per_crate_kinds
            .entry(crate_name.to_string())
            .or_default()
            .entry("type_usage".to_string())
            .or_default() += 1;
    }
    let mut per_crate_top_kind: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (crate_name, counter) in &per_crate_kinds {
        // Deterministic election (count desc, kind name asc on ties):
        // HashMap iteration order randomized tied winners between
        // runs, drifting distinct_top_kinds / dispersion across
        // identical-input characterize invocations (the sampling
        // contract requires byte-reproducible output).
        let mut ranked: Vec<(&String, &usize)> = counter.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if let Some((kind, _)) = ranked.first() {
            per_crate_top_kind.insert(crate_name.clone(), (*kind).clone());
        }
    }
    let distinct_top_kinds: std::collections::HashSet<String> =
        per_crate_top_kind.values().cloned().collect();
    let kind_dominance_dispersion = if !per_crate_top_kind.is_empty() {
        distinct_top_kinds.len() as f64 / per_crate_top_kind.len() as f64
    } else {
        0.0
    };

    let workspace_crate_set: std::collections::HashSet<String> =
        crate_names.iter().cloned().collect();
    let mut dependent_count: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (_name, info) in per_crate.iter() {
        for dep in &info.deps {
            if workspace_crate_set.contains(dep) {
                *dependent_count.entry(dep.clone()).or_default() += 1;
            }
        }
    }
    let leaf_crates = crate_names
        .iter()
        .filter(|name| dependent_count.get(*name).copied().unwrap_or(0) == 0)
        .count();
    let leaf_ratio = leaf_crates as f64 / n_crates as f64;
    let max_dependents = dependent_count.values().max().copied().unwrap_or(0);
    let hub_centrality = max_dependents as f64 / n_crates as f64;

    let central_crate: Option<String> = {
        // Deterministic election (dependents desc, crate name asc on
        // ties): bevy_ecs and bevy_reflect tie at 44 dependents, and
        // the prior HashMap-order pick flipped central_crate between
        // identical-input runs.
        let mut ranked: Vec<(&String, &usize)> = dependent_count.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        ranked.first().map(|(k, _)| (*k).clone())
    };
    let central_kind: Option<String> = central_crate
        .as_ref()
        .and_then(|c| per_crate_top_kind.get(c).cloned());

    let mut signals: indexmap::IndexMap<String, serde_json::Value> = indexmap::IndexMap::new();
    signals.insert(
        "uniqueness_ratio".to_string(),
        serde_json::Value::from(round3(uniqueness_ratio)),
    );
    signals.insert(
        "kind_dominance_dispersion".to_string(),
        serde_json::Value::from(round3(kind_dominance_dispersion)),
    );
    signals.insert(
        "leaf_ratio".to_string(),
        serde_json::Value::from(round3(leaf_ratio)),
    );
    signals.insert(
        "hub_centrality".to_string(),
        serde_json::Value::from(round3(hub_centrality)),
    );
    signals.insert(
        "central_crate".to_string(),
        match central_crate {
            Some(c) => serde_json::Value::String(c),
            None => serde_json::Value::Null,
        },
    );
    signals.insert(
        "central_kind".to_string(),
        match central_kind {
            Some(c) => serde_json::Value::String(c),
            None => serde_json::Value::Null,
        },
    );
    signals.insert(
        "n_crates".to_string(),
        serde_json::Value::from(crate_names.len()),
    );
    signals.insert(
        "unique_patterns".to_string(),
        serde_json::Value::from(unique_patterns),
    );
    signals.insert(
        "single_crate_patterns".to_string(),
        serde_json::Value::from(single_crate_patterns),
    );
    signals.insert(
        "distinct_top_kinds".to_string(),
        serde_json::Value::from(distinct_top_kinds.len()),
    );
    signals
}

fn round3(x: f64) -> f64 {
    format!("{:.3}", x).parse().unwrap_or(x)
}
