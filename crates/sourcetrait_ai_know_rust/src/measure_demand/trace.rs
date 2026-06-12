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
    consumer_renames: &std::collections::HashMap<String, String>,
    target_facts: &serde_json::Value,
    target_fp: &serde_json::Value,
    target_orientation: &str,
    overlay_fn_paths: &std::collections::HashMap<String, Vec<String>>,
    adopted_decls: &[(String, String)],
) -> DemandReport {
    // Target vocabulary: package bindings plus lib-rename bindings
    // (names are bindings; `use cosmic::` must reach package
    // libcosmic). Absent lib_name fields (pre-identity fingerprints)
    // degrade to package-only.
    let target_crates: std::collections::BTreeSet<String> = target_fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .flat_map(|(k, v)| {
                    let mut names = vec![demand_norm(k)];
                    if let Some(ln) = v.get("lib_name").and_then(|x| x.as_str()) {
                        names.push(demand_norm(ln));
                    }
                    names
                })
                .collect()
        })
        .unwrap_or_default();

    // Target declaration sets: name -> kinds, plus the module-name
    // set for the namespace bucket.
    let mut decl: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    add_decls(target_facts, "types", "type", &mut decl);
    add_decls(target_facts, "traits", "trait", &mut decl);
    add_decls(target_facts, "fns", "fn", &mut decl);
    add_decls(target_facts, "macro_defs", "macro", &mut decl);
    // Adopted surface items are target API (the adoption rule):
    // real kinds, and the mod-namespace guard sees them as item
    // decls.
    for (n, k) in adopted_decls {
        decl.entry(n.clone()).or_default().insert(k.clone());
    }
    let mods: std::collections::BTreeSet<String> = target_facts
        .get("mods")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter(|m| {
                    !is_example_path(m.get("file").and_then(|v| v.as_str()).unwrap_or(""))
                })
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .filter(|n| !n.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    // Foreign re-export surface: the target's `pub use` facts whose
    // ROOT is neither a workspace crate nor a self-reference root
    // expose FOREIGN items/namespaces on the target's API - std /
    // core / alloc COUNT as foreign (tokio's `pub use
    // std::time::Duration` is exactly the class: real surface no
    // workspace pick can serve). Leaf bindings (pub use mime::Mime
    // [as X]) collect by their EXPOSED name; single-segment
    // re-exports (pub use futures;) plus every foreign root form
    // the namespace set the demand-root/parent match consults.
    // Demands served only by these classify into the non-gating
    // foreign_reexport bucket.
    let lang_roots = ["crate", "self", "super"];
    // Local-module gate: a uniform-path re-export (`pub use
    // action::Action;` beside `mod action;`) roots at the module in
    // SCOPE, not a foreign crate. Uniform-path resolution is
    // per-module-scope, so the gate matches mod decls at the SAME
    // (crate, file, inline module chain) as the use fact - a
    // crate-wide name match over-gates (tokio's nested loom `std`
    // shim must not shadow `pub use std::time::Duration` written in
    // another file).
    let mut local_mod_scopes: std::collections::HashSet<(String, String, String, String)> =
        std::collections::HashSet::new();
    if let Some(arr) = target_facts.get("mods").and_then(|v| v.as_array()) {
        for m in arr {
            if let (Some(n), Some(c), Some(f)) = (
                m.get("name").and_then(|v| v.as_str()),
                m.get("crate").and_then(|v| v.as_str()),
                m.get("file").and_then(|v| v.as_str()),
            ) {
                let mp = m
                    .get("module_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                local_mod_scopes.insert((
                    c.to_string(),
                    f.to_string(),
                    mp,
                    n.to_string(),
                ));
            }
        }
    }
    let mut foreign_leafs: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut foreign_ns: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    if let Some(uses) = target_facts.get("uses").and_then(|v| v.as_array()) {
        for u in uses {
            if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
                continue;
            }
            // Example-file re-exports never expose foreign surface
            // (the example-origin rule).
            if is_example_path(u.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                continue;
            }
            let Some(path) = u.get("path").and_then(|v| v.as_str()) else {
                continue;
            };
            let parsed = parse_use_leaves(path);
            if parsed.root.is_empty()
                || lang_roots.contains(&parsed.root.as_str())
                || target_crates.contains(&demand_norm(&parsed.root))
            {
                continue;
            }
            // An explicit-external path (leading `::`) names the
            // foreign crate by language semantics; the local-module
            // gate never applies to it.
            let local_mod = if parsed.explicit_external {
                false
            } else {
                match (
                    u.get("crate").and_then(|v| v.as_str()),
                    u.get("file").and_then(|v| v.as_str()),
                ) {
                    (Some(c), Some(f)) => {
                        let mp = u
                            .get("module_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        local_mod_scopes.contains(&(
                            c.to_string(),
                            f.to_string(),
                            mp,
                            parsed.root.clone(),
                        ))
                    }
                    _ => false,
                }
            };
            if local_mod {
                continue;
            }
            foreign_ns.insert(demand_norm(&parsed.root));
            let path_stripped = path.trim().strip_prefix("::").unwrap_or(path.trim());
            let crate_level = !path_stripped.contains("::");
            for leaf in &parsed.leaves {
                if let UseLeaf::Named { binding, .. } = leaf {
                    foreign_leafs.insert(binding.clone());
                    if crate_level {
                        foreign_ns.insert(demand_norm(binding));
                    }
                }
            }
        }
    }

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
    // Consumer-local module gate (the demand-side mirror of the
    // capture/5F local-module gates): a use-import whose ROOT names
    // a module declared in the SAME (file, inline module chain) is
    // uniform-path-local to the consumer, never target demand
    // (nu-jupyter-kernel's `use nu::konst::Konst;` beside `mod nu;`
    // must not resolve to nushell's `nu` binary crate). Use-path
    // resolution is same-module + extern prelude - not lexical
    // ancestors - so same-(file, chain) matching is the language
    // semantics, exactly like the capture gate. Qualified usage
    // sites and per-file bindings gate at FILE level only (the
    // usage wire carries no inline chain); the approximation is
    // bounded to same-file name collisions.
    let mut local_use_scopes: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    let mut local_mod_files: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for m in &consumer_items.mods {
        let mp = m.module_path.clone().unwrap_or_default();
        local_use_scopes.insert((m.file.clone(), mp, m.name.clone()));
        local_mod_files.insert((m.file.clone(), m.name.clone()));
    }
    let mut alias_global: std::collections::HashMap<String, ImportBinding> =
        std::collections::HashMap::new();

    let mut demand: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    // Per demanded name / pair: consumer site counts (every stream
    // insertion is one site) - the weight blob's magnitude signal.
    let mut demand_sites: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut pair_sites: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    // Per demanded name: the normalized target ROOT bindings the
    // demand resolved through. The end-product criterion ("would the
    // reading agent find it in the bundle") makes `<root>::<name>`
    // pick pairs serve the name - tokio's macro-wrapped API surfaces
    // ONLY as `implementation_functions:tokio::spawn`-shaped picks,
    // and a consumer's `tokio::spawn(...)` demand must match them.
    let mut demand_roots: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut pairs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut globs: Vec<String> = Vec::new();

    for u in &consumer_items.uses {
        let parsed = parse_use_leaves(&u.path);
        if parsed.root.is_empty() {
            continue;
        }
        // Uniform-path local import: consumer-internal, never target
        // demand; its renamed leaves must not seed the crate-wide
        // alias map either. An explicit-external path (leading `::`)
        // bypasses the gate by language semantics.
        if !parsed.explicit_external
            && local_use_scopes.contains(&(
                u.file.clone(),
                u.module_path.clone().unwrap_or_default(),
                parsed.root.clone(),
            ))
        {
            continue;
        }
        for leaf in &parsed.leaves {
            if let UseLeaf::Named { binding, source, .. } = leaf {
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
        if root_in_targets(&parsed.root, consumer_renames, &target_crates) {
            let kind = if u.reexport { "reexport" } else { "use" };
            for leaf in &parsed.leaves {
                match leaf {
                    UseLeaf::Glob { .. } => globs.push(u.path.clone()),
                    UseLeaf::Named { binding, source, parent } => {
                        // Demand records the SOURCE name; the binding
                        // is only the consumer's local spelling. The
                        // leaf's PARENT segment joins the root set:
                        // module-fn picks are keyed `<module>::<fn>`
                        // (`use iced::border::radius;` is served by
                        // `implementation_functions:border::radius`).
                        let nm = source
                            .clone()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| binding.clone());
                        for r in root_candidates(&parsed.root, consumer_renames) {
                            demand_roots.entry(nm.clone()).or_default().insert(r);
                        }
                        if let Some(p) = parent {
                            demand_roots
                                .entry(nm.clone())
                                .or_default()
                                .insert(p.clone());
                        }
                        *demand_sites.entry(nm.clone()).or_default() += 1;
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
        if let Some((src, root)) = demand_resolved(
            file,
            ident,
            qualifier.as_deref(),
            &imap,
            &alias_global,
            consumer_renames,
            &target_crates,
            &local_mod_files,
        ) {
            for r in root_candidates(&root, consumer_renames) {
                demand_roots.entry(src.clone()).or_default().insert(r);
            }
            // The written qualifier joins the root set as a literal
            // spelling: when the qualifier resolved through a
            // binding (use tokio::io; io::copy(..)), the written
            // module segment is the spelling rendered pair picks
            // are keyed under.
            if let Some(q) = qualifier.as_deref() {
                demand_roots.entry(src.clone()).or_default().insert(q.to_string());
            }
            *demand_sites.entry(src.clone()).or_default() += 1;
            demand.entry(src).or_default().insert("ident".to_string());
        }
    }
    for e in &consumer_usages.ast_fn_call_usages {
        if let Some((src, root)) = demand_resolved(
            &e.file,
            &e.name,
            e.qualifier.as_deref(),
            &imap,
            &alias_global,
            consumer_renames,
            &target_crates,
            &local_mod_files,
        ) {
            for r in root_candidates(&root, consumer_renames) {
                demand_roots.entry(src.clone()).or_default().insert(r);
            }
            // The call-site parent joins the root set the same way
            // the import-leaf parent does: a full-path call
            // (`cosmic::iced::stream::channel(..)`) is served by the
            // rendered `stream::channel` pair pick.
            if let Some(p) = &e.parent {
                demand_roots
                    .entry(src.clone())
                    .or_default()
                    .insert(p.clone());
            }
            if let Some(q) = e.qualifier.as_deref() {
                demand_roots.entry(src.clone()).or_default().insert(q.to_string());
            }
            *demand_sites.entry(src.clone()).or_default() += 1;
            demand.entry(src).or_default().insert("fn_call".to_string());
        }
    }
    for e in &consumer_usages.ast_method_ref_usages {
        if let Some((src, root)) = demand_resolved(
            &e.file,
            &e.outer,
            e.qualifier.as_deref(),
            &imap,
            &alias_global,
            consumer_renames,
            &target_crates,
            &local_mod_files,
        ) {
            for r in root_candidates(&root, consumer_renames) {
                demand_roots.entry(src.clone()).or_default().insert(r);
            }
            if let Some(q) = e.qualifier.as_deref() {
                demand_roots.entry(src.clone()).or_default().insert(q.to_string());
            }
            *demand_sites.entry(src.clone()).or_default() += 1;
            demand
                .entry(src.clone())
                .or_default()
                .insert("method_ref".to_string());
            let pair_key = format!("{}::{}", src, e.inner);
            *pair_sites.entry(pair_key.clone()).or_default() += 1;
            pairs
                .entry(pair_key)
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
        if let Some((src, root)) = demand_resolved(
            &t.file,
            o,
            t.qualifier.as_deref(),
            &imap,
            &alias_global,
            consumer_renames,
            &target_crates,
            &local_mod_files,
        ) {
            for r in root_candidates(&root, consumer_renames) {
                demand_roots.entry(src.clone()).or_default().insert(r);
            }
            if let Some(q) = t.qualifier.as_deref() {
                demand_roots.entry(src.clone()).or_default().insert(q.to_string());
            }
            *demand_sites.entry(src.clone()).or_default() += 1;
            demand
                .entry(src.clone())
                .or_default()
                .insert("type_usage".to_string());
            let pair_key = format!("{}::{}", src, i);
            *pair_sites.entry(pair_key.clone()).or_default() += 1;
            pairs
                .entry(pair_key)
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

    // Alternate binding outers per rendered pair (the hard+soft item
    // model): a demand reaching ANY public spelling of an item is
    // served by the one rendered key.
    let mut alias_outers_by_pair: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    if let Some(aliases) = target_facts.get("pair_aliases").and_then(|v| v.as_object()) {
        for (pair, outers) in aliases {
            if let Some(arr) = outers.as_array() {
                alias_outers_by_pair.insert(
                    pair.clone(),
                    arr.iter()
                        .filter_map(|o| o.as_str().map(String::from))
                        .collect(),
                );
            }
        }
    }
    // Rendered pairs indexed by inner name for the alias consult.
    let mut rendered_by_inner: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for p in &pick_pairs {
        if let Some((_, inner)) = p.split_once("::") {
            rendered_by_inner
                .entry(inner.to_string())
                .or_default()
                .push(p.clone());
        }
    }
    // Overlay paths join the alias union: for each rendered pair
    // whose inner has rustdoc canonical paths, the paths' penultimate
    // segments become additional consultable outers (name-level
    // granularity; same-name collision risk is the system's standing
    // trade). Closes the glob-re-export alias gap.
    for (inner, rendered) in &rendered_by_inner {
        let Some(paths) = overlay_fn_paths.get(inner) else {
            continue;
        };
        let penults: Vec<String> = paths
            .iter()
            .filter_map(|p| {
                let segs: Vec<&str> = p.split("::").collect();
                if segs.len() >= 2 {
                    Some(segs[segs.len() - 2].to_string())
                } else {
                    None
                }
            })
            .collect();
        if penults.is_empty() {
            continue;
        }
        for rp in rendered {
            let entry = alias_outers_by_pair.entry(rp.clone()).or_default();
            for p in &penults {
                if !entry.contains(p) {
                    entry.push(p.clone());
                }
            }
        }
    }

    let mut mod_ns: Vec<DemandRecord> = Vec::new();
    let mut hits: Vec<DemandRecord> = Vec::new();
    let mut misses: Vec<DemandRecord> = Vec::new();
    let mut foreign: Vec<DemandRecord> = Vec::new();
    for (nm, srcs) in &demand {
        let kinds: Vec<String> = match decl.get(nm) {
            Some(k) if !k.is_empty() => k.iter().cloned().collect(),
            _ => vec!["unknown".to_string()],
        };
        let rec = DemandRecord {
            name: nm.clone(),
            kinds,
            srcs: srcs.iter().cloned().collect(),
            sites: demand_sites.get(nm).copied().unwrap_or(0),
        };
        // End-product criterion: a name is served when the bundle
        // presents it - by its own name in picks/carry, OR as the
        // inner of a rendered `<root>::<name>` pair pick under a
        // target root the demand resolved through (tokio::spawn).
        let root_pair_served = demand_roots
            .get(nm)
            .map(|roots| {
                roots
                    .iter()
                    .any(|r| pick_pairs.contains(&format!("{}::{}", r, nm)))
                    || rendered_by_inner.get(nm).map_or(false, |pairs| {
                        pairs.iter().any(|p| {
                            alias_outers_by_pair
                                .get(p)
                                .map_or(false, |outers| {
                                    outers.iter().any(|o| roots.contains(o))
                                })
                        })
                    })
            })
            .unwrap_or(false);
        // Foreign classification fires only when nothing in the
        // bundle serves the name (a served name is a hit regardless
        // of how the target also re-exports foreign material).
        let foreign_served = foreign_leafs.contains(nm)
            || demand_roots
                .get(nm)
                .map(|roots| roots.iter().any(|r| foreign_ns.contains(&demand_norm(r))))
                .unwrap_or(false);
        // A demanded name that IS a target crate binding is a
        // crate-NAMESPACE import (`use tokio as tk;` / `pub use
        // ratatui;`) - a binding to the whole surface, not an item
        // demand; the namespace bucket is its home (non-gating).
        if !decl.contains_key(nm)
            && (mods.contains(nm) || target_crates.contains(&demand_norm(nm)))
        {
            mod_ns.push(rec);
        } else if covered.contains(nm) || root_pair_served {
            hits.push(rec);
        } else if foreign_served {
            foreign.push(rec);
        } else {
            misses.push(rec);
        }
    }

    let mut pair_exact: Vec<String> = Vec::new();
    let mut pair_name_level: Vec<String> = Vec::new();
    let mut pair_misses: Vec<String> = Vec::new();
    let mut pair_foreign: Vec<String> = Vec::new();
    for p in pairs.keys() {
        let (o, i) = match p.split_once("::") {
            Some((o, i)) => (o, i),
            None => (p.as_str(), ""),
        };
        // Exact under the rendered spelling OR under any alternate
        // binding outer of a rendered pair with the same inner.
        let alias_exact = rendered_by_inner.get(i).map_or(false, |pairs| {
            pairs.iter().any(|rp| {
                alias_outers_by_pair
                    .get(rp)
                    .map_or(false, |outers| outers.iter().any(|a| a == o))
            })
        });
        if pick_pairs.contains(p) || alias_exact {
            pair_exact.push(p.clone());
        } else if covered.contains(o) {
            pair_name_level.push(p.clone());
        } else if target_crates.contains(&demand_norm(o)) && covered.contains(i) {
            // A crate-qualified spelling of a covered bare name
            // (`nu::reg()` with `utilities:reg` rendered): the pair
            // IS the name demand under crate qualification - served
            // name-level, mirroring the crate-namespace rule on the
            // name tier.
            pair_name_level.push(p.clone());
        } else if foreign_ns.contains(&demand_norm(o)) || foreign_leafs.contains(o) {
            // Foreign-outer pairs mirror the name bucket: real
            // demand, structurally unservable by workspace picks.
            pair_foreign.push(p.clone());
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
        foreign_reexport_count: foreign.len(),
        foreign_reexports: foreign,
        pairs_total: pairs.len(),
        pair_exact: pair_exact.len(),
        pair_name_level: pair_name_level.len(),
        pair_miss_count: pair_misses.len(),
        pair_misses,
        pair_foreign,
        globs,
    };
    DemandReport {
        summary,
        hits,
        mod_namespace: mod_ns,
        pair_name_level,
        pair_sites,
    }
}

/// What: hyphen-to-underscore crate-name normalization (cargo names
/// vs path roots).
fn demand_norm(s: &str) -> String {
    s.replace('-', "_")
}

/// What: true when a source ROOT binding reaches a target crate -
/// directly, or through the consumer's dependency-rename map
/// (`tk = { package = "tgt-kit" }` makes `tk::` a target root).
///
/// Why: names are bindings; the consumer's manifests define which
/// bindings mean which packages, and the demand side must speak the
/// same vocabulary the capture side does.
///
/// Where: called by the use-import demand loop and `demand_resolved`.
fn root_in_targets(
    root: &str,
    consumer_renames: &std::collections::HashMap<String, String>,
    target_crates: &std::collections::BTreeSet<String>,
) -> bool {
    let n = demand_norm(root);
    if target_crates.contains(&n) {
        return true;
    }
    consumer_renames
        .get(&n)
        .map(|pkg| target_crates.contains(&demand_norm(pkg)))
        .unwrap_or(false)
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
            // Example-declared items cannot serve demand (the
            // example-origin rule): they never enter the target
            // vocabulary.
            if is_example_path(t.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                continue;
            }
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
/// a target crate. Returns the SOURCE name (rename-translated) plus
/// the ROOT binding the demand resolved through, or None -
/// name-keyed declaration matches alone are NOT inclusion.
///
/// Why: demand's credit rule is stricter than the capture side's
/// `site_credits` (which lets the Unresolved crate-local fallback
/// through; a bare unimported name on the consumer side is the
/// consumer's own). The rule reads the shared `ImportBinding`
/// surface so the resolution DATA is identical on both sides; only
/// the verdict differs. The root rides along because the coverage
/// check needs `<root>::<name>` pair-pick lookups (the end-product
/// criterion: a `tokio::spawn(...)` demand is served by the
/// `implementation_functions:tokio::spawn` pick).
///
/// Where: called per usage-site stream in `demand_report`.
#[allow(clippy::too_many_arguments)]
fn demand_resolved(
    file: &str,
    name: &str,
    qualifier: Option<&str>,
    imap: &ImportMaps,
    alias_global: &std::collections::HashMap<String, ImportBinding>,
    consumer_renames: &std::collections::HashMap<String, String>,
    target_crates: &std::collections::BTreeSet<String>,
    local_mod_files: &std::collections::HashSet<(String, String)>,
) -> Option<(String, String)> {
    if let Some(q) = qualifier {
        // A written qualifier is a BINDING, not necessarily a crate
        // name. Resolve it through this file's own import surface
        // first (`use crate::nu;` makes a qualified `nu::execute(..)`
        // consumer-internal even when a target crate is named `nu`;
        // `use tokio as tk;` makes `tk::spawn(..)` real target
        // demand), then through the same-file local-module gate
        // (uniform paths), then as a directly-written root. The
        // name-binding/alias fallbacks below still apply on
        // fall-through: a local module may re-export a target item
        // (the consumer-internal re-export chain the alias map
        // serves).
        let q_effective: Option<String> = match imap
            .get(file)
            .and_then(|m| m.get(q))
            .map(|b| b.root.clone())
        {
            Some(r) if ["crate", "self", "super"].contains(&r.as_str()) => None,
            Some(r) if local_mod_files.contains(&(file.to_string(), r.clone())) => None,
            Some(r) => Some(r),
            None if local_mod_files.contains(&(file.to_string(), q.to_string())) => None,
            None => Some(q.to_string()),
        };
        if let Some(qr) = q_effective {
            if root_in_targets(&qr, consumer_renames, target_crates) {
                if let Some(b) = alias_global.get(name) {
                    if root_in_targets(&b.root, consumer_renames, target_crates) {
                        return b.source.clone().map(|s| (s, b.root.clone()));
                    }
                }
                return Some((name.to_string(), qr));
            }
        }
    }
    if let Some(b) = imap.get(file).and_then(|m| m.get(name)) {
        if !local_mod_files.contains(&(file.to_string(), b.root.clone()))
            && root_in_targets(&b.root, consumer_renames, target_crates)
        {
            return Some((
                b.source
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| name.to_string()),
                b.root.clone(),
            ));
        }
    }
    if let Some(b) = alias_global.get(name) {
        if root_in_targets(&b.root, consumer_renames, target_crates) {
            return b.source.clone().map(|s| (s, b.root.clone()));
        }
    }
    None
}

/// What: the normalized target-binding forms a demand ROOT can match
/// pick-pair outers under: the root itself plus its dependency-rename
/// translation when one exists. Rust path segments never contain
/// hyphens, so normalized forms compare directly against written pick
/// outers.
///
/// Why: the bundle's `<outer>::<name>` pair picks are written in the
/// TARGET's own source spelling; a consumer demanding through a
/// renamed binding (`tk = { package = "tokio" }`) must look the pair
/// up under the package binding too.
///
/// Where: called per demand insertion in `demand_report` to populate
/// the name -> roots map the coverage check consults.
fn root_candidates(
    root: &str,
    consumer_renames: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let n = demand_norm(root);
    let mut out = vec![n.clone()];
    if let Some(pkg) = consumer_renames.get(&n) {
        let p = demand_norm(pkg);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}
