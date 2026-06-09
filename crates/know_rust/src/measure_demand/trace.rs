use crate::*;

/// What: build the demand-trace report for one (consumer, target)
/// pair: every consumer site that affirmatively resolves to a target
/// crate becomes demand; coverage is the target's rendered picks
/// union plus carry names; pairs report exact / name-level / miss
/// tiers.
///
/// Why: the demand-side quality gate behind the zero-miss
/// consumer-trace bar (working/11). The consumer's import surface is
/// read through the shared resolution module - the same grammar and
/// `ImportBinding` maps the capture side resolves with - and the
/// rendered-pick coverage comes from measure_overlap's S5 parser, so
/// the three consumers of those grammars cannot drift apart. Demand
/// keeps its own credit rule on top of the shared substrate:
/// affirmative resolution to a target crate only (Unresolved is NOT
/// demand, unlike the capture side's crate-local fallback).
///
/// Where: called by `measure_demand::run::measure_demand` with the
/// consumer's in-process scan outputs and the target's loaded
/// artifacts.
pub(crate) fn demand_report(
    consumer_items: &ItemFacts,
    consumer_usages: &UsageFacts,
    target_facts: &serde_json::Value,
    target_fp: &serde_json::Value,
    target_orientation: &str,
) -> DemandReport {
    let target_crates: std::collections::BTreeSet<String> = target_fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().map(|k| demand_norm(k)).collect())
        .unwrap_or_default();

    // Target declaration sets: name -> kinds, plus the module-name
    // set for the namespace bucket.
    let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    add_decls(target_facts, "types", "type", &mut decl);
    add_decls(target_facts, "traits", "trait", &mut decl);
    add_decls(target_facts, "fns", "fn", &mut decl);
    add_decls(target_facts, "macro_defs", "macro", &mut decl);
    let mods: std::collections::BTreeSet<String> = target_facts
        .get("mods")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .filter(|n| !n.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    // Consumer import surface: the per-file binding maps come from
    // the shared builder (the same one pattern_metrics uses); the
    // crate-wide rename map - demand's own concept, for consumer-
    // internal re-export chains (alias declared in one file, used via
    // an internal module path in another) - is built from the same
    // parsed leaves, first-seen-wins.
    let imap: ImportMaps = build_import_bindings(
        consumer_items
            .uses
            .iter()
            .map(|u| (u.file.as_str(), u.path.as_str())),
    );
    let mut alias_global: std::collections::HashMap<String, ImportBinding> =
        std::collections::HashMap::new();

    let mut demand: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut pairs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut globs: Vec<String> = Vec::new();

    for u in &consumer_items.uses {
        let parsed = parse_use_leaves(&u.path);
        if parsed.root.is_empty() {
            continue;
        }
        for leaf in &parsed.leaves {
            if let UseLeaf::Named { binding, source } = leaf {
                if let Some(s) = source {
                    if !s.is_empty() && s != binding {
                        alias_global
                            .entry(binding.clone())
                            .or_insert_with(|| ImportBinding {
                                root: parsed.root.clone(),
                                source: Some(s.clone()),
                            });
                    }
                }
            }
        }
        if target_crates.contains(&demand_norm(&parsed.root)) {
            let kind = if u.reexport { "reexport" } else { "use" };
            for leaf in &parsed.leaves {
                match leaf {
                    UseLeaf::Glob => globs.push(u.path.clone()),
                    UseLeaf::Named { binding, source } => {
                        // Demand records the SOURCE name; the binding
                        // is only the consumer's local spelling.
                        let nm = source
                            .clone()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| binding.clone());
                        demand.entry(nm).or_default().insert(kind.to_string());
                    }
                }
            }
        }
    }

    // Ident streams (sig / field / alias), call heads, method refs,
    // and type usages - each included only on affirmative resolution
    // to a target crate, with rename translation back to the source
    // name.
    for e in consumer_usages
        .ast_fn_sig_usages
        .iter()
        .map(|e| (&e.file, &e.ident, &e.qualifier))
        .chain(
            consumer_usages
                .ast_field_usages
                .iter()
                .map(|e| (&e.file, &e.ident, &e.qualifier)),
        )
        .chain(
            consumer_usages
                .ast_type_alias_usages
                .iter()
                .map(|e| (&e.file, &e.ident, &e.qualifier)),
        )
    {
        let (file, ident, qualifier) = e;
        if let Some(src) = demand_resolved(
            file,
            ident,
            qualifier.as_deref(),
            &imap,
            &alias_global,
            &target_crates,
        ) {
            demand.entry(src).or_default().insert("ident".to_string());
        }
    }
    for e in &consumer_usages.ast_fn_call_usages {
        if let Some(src) = demand_resolved(
            &e.file,
            &e.name,
            e.qualifier.as_deref(),
            &imap,
            &alias_global,
            &target_crates,
        ) {
            demand.entry(src).or_default().insert("fn_call".to_string());
        }
    }
    for e in &consumer_usages.ast_method_ref_usages {
        if let Some(src) = demand_resolved(
            &e.file,
            &e.outer,
            e.qualifier.as_deref(),
            &imap,
            &alias_global,
            &target_crates,
        ) {
            demand
                .entry(src.clone())
                .or_default()
                .insert("method_ref".to_string());
            pairs
                .entry(format!("{}::{}", src, e.inner))
                .or_default()
                .insert("method_ref".to_string());
        }
    }
    for t in consumer_items
        .type_usages
        .iter()
        .chain(consumer_items.example_type_usages.iter())
    {
        let Some((o, i)) = t.name.split_once("::") else {
            continue;
        };
        if let Some(src) = demand_resolved(
            &t.file,
            o,
            t.qualifier.as_deref(),
            &imap,
            &alias_global,
            &target_crates,
        ) {
            demand
                .entry(src.clone())
                .or_default()
                .insert("type_usage".to_string());
            pairs
                .entry(format!("{}::{}", src, i))
                .or_default()
                .insert("type_usage".to_string());
        }
    }

    // Coverage: rendered S5 picks via the shared parser (pair keys
    // cover their outer) plus carry keys and carried names. The pick
    // strings convert to typed Patterns at this boundary - the same
    // one-string-parse-boundary idiom the picker uses - and the pair
    // vs name split reads the typed name (a Globals pick like
    // `globals:Status::ACTIVE` is a real pair).
    let picks = parse_picks(target_orientation);
    let mut pick_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pick_pairs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for pick in picks.all() {
        let Some(pattern) = Pattern::from_wire(&pick) else {
            continue;
        };
        let name = pattern.name();
        match name.split_once("::") {
            Some((outer, _)) => {
                pick_pairs.insert(name.clone());
                if outer != "_" {
                    pick_names.insert(outer.to_string());
                }
            }
            None => {
                pick_names.insert(name);
            }
        }
    }
    let mut carry_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(carries) = target_facts.get("carries").and_then(|v| v.as_object()) {
        for (key, entries) in carries {
            if let Some(pattern) = Pattern::from_wire(key) {
                let name = pattern.name();
                let outer = name.split_once("::").map(|(o, _)| o).unwrap_or(name.as_str());
                carry_names.insert(outer.to_string());
            }
            if let Some(arr) = entries.as_array() {
                for e in arr {
                    if let Some(n) = e.get("name").and_then(|v| v.as_str()) {
                        if !n.is_empty() {
                            carry_names.insert(n.to_string());
                        }
                    }
                }
            }
        }
    }
    let covered: std::collections::BTreeSet<String> =
        pick_names.union(&carry_names).cloned().collect();

    let mut mod_ns: Vec<DemandRecord> = Vec::new();
    let mut hits: Vec<DemandRecord> = Vec::new();
    let mut misses: Vec<DemandRecord> = Vec::new();
    for (nm, srcs) in &demand {
        let kinds: Vec<String> = match decl.get(nm) {
            Some(k) if !k.is_empty() => k.iter().cloned().collect(),
            _ => vec!["unknown".to_string()],
        };
        let rec = DemandRecord {
            name: nm.clone(),
            kinds,
            srcs: srcs.iter().cloned().collect(),
        };
        if !decl.contains_key(nm) && mods.contains(nm) {
            mod_ns.push(rec);
        } else if covered.contains(nm) {
            hits.push(rec);
        } else {
            misses.push(rec);
        }
    }

    let mut pair_exact: Vec<String> = Vec::new();
    let mut pair_name_level: Vec<String> = Vec::new();
    let mut pair_misses: Vec<String> = Vec::new();
    for p in pairs.keys() {
        let o = p.split_once("::").map(|(o, _)| o).unwrap_or(p);
        if pick_pairs.contains(p) {
            pair_exact.push(p.clone());
        } else if covered.contains(o) {
            pair_name_level.push(p.clone());
        } else {
            pair_misses.push(p.clone());
        }
    }

    let summary = DemandSummary {
        demanded_names: demand.len(),
        hits: hits.len(),
        miss_count: misses.len(),
        misses: misses.clone(),
        mod_namespace_count: mod_ns.len(),
        pairs_total: pairs.len(),
        pair_exact: pair_exact.len(),
        pair_name_level: pair_name_level.len(),
        pair_miss_count: pair_misses.len(),
        pair_misses,
        globs,
    };
    DemandReport {
        summary,
        hits,
        mod_namespace: mod_ns,
        pair_name_level,
    }
}

/// What: hyphen-to-underscore crate-name normalization (cargo names
/// vs path roots).
fn demand_norm(s: &str) -> String {
    s.replace('-', "_")
}

/// What: fold one target fact list's `name` fields into the
/// declaration map under the given kind tag.
fn add_decls(
    facts: &serde_json::Value,
    list: &str,
    kind: &str,
    decl: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
) {
    if let Some(arr) = facts.get(list).and_then(|v| v.as_array()) {
        for t in arr {
            if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
                if !n.is_empty() {
                    decl.entry(n.to_string())
                        .or_default()
                        .insert(kind.to_string());
                }
            }
        }
    }
}

/// What: affirmative resolution of a consumer site to a target
/// crate: an explicit target-crate qualifier, a per-file import
/// binding to a target crate, or a crate-wide rename whose source is
/// a target crate. Returns the SOURCE name (rename-translated) or
/// None - name-keyed declaration matches alone are NOT inclusion.
///
/// Why: demand's credit rule is stricter than the capture side's
/// `site_credits` (which lets the Unresolved crate-local fallback
/// through; a bare unimported name on the consumer side is the
/// consumer's own). The rule reads the shared `ImportBinding`
/// surface so the resolution DATA is identical on both sides; only
/// the verdict differs.
///
/// Where: called per usage-site stream in `demand_report`.
fn demand_resolved(
    file: &str,
    name: &str,
    qualifier: Option<&str>,
    imap: &ImportMaps,
    alias_global: &std::collections::HashMap<String, ImportBinding>,
    target_crates: &std::collections::BTreeSet<String>,
) -> Option<String> {
    if let Some(q) = qualifier {
        if target_crates.contains(&demand_norm(q)) {
            if let Some(b) = alias_global.get(name) {
                if target_crates.contains(&demand_norm(&b.root)) {
                    return b.source.clone();
                }
            }
            return Some(name.to_string());
        }
    }
    if let Some(b) = imap.get(file).and_then(|m| m.get(name)) {
        if target_crates.contains(&demand_norm(&b.root)) {
            return Some(
                b.source
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| name.to_string()),
            );
        }
    }
    if let Some(b) = alias_global.get(name) {
        if target_crates.contains(&demand_norm(&b.root)) {
            return b.source.clone();
        }
    }
    None
}
