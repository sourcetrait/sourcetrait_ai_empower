use crate::*;

/// What: build the demand-trace report for one (consumer, target)
/// pair: every consumer site that affirmatively resolves to a target
/// crate becomes demand; coverage is the target's rendered picks
/// union plus carry names; pairs report exact / name-level / miss
/// tiers.
///
/// Why: phase-1 language conversion of scripts/py/kr_consumer_trace.py
/// (the prototype behind the zero-miss consumer-trace bar in
/// working/11). The logic mirrors the python function-for-function -
/// including its own use-tree parsing and site resolution, which
/// deliberately DUPLICATE pattern_metrics' machinery; unification is
/// the phase-2 partner rewrite.
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

    // Consumer import maps: per-file binding -> (root, source), plus
    // the crate-wide rename map for consumer-internal re-export
    // chains (alias declared in one file, used via an internal module
    // path in another).
    let mut demand: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut pairs: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut globs: Vec<String> = Vec::new();
    let mut imap: std::collections::HashMap<
        String,
        std::collections::HashMap<String, (String, Option<String>)>,
    > = std::collections::HashMap::new();
    let mut alias_global: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();

    for u in &consumer_items.uses {
        let first = u.path.split("::").next().unwrap_or("").trim().to_string();
        let mut lv: Vec<(Option<String>, Option<String>)> = Vec::new();
        demand_leaves(&u.path, None, &mut lv);
        let fm = imap.entry(u.file.clone()).or_default();
        for (binding, source) in &lv {
            if let Some(b) = binding {
                if b != "*" {
                    fm.entry(b.clone())
                        .or_insert_with(|| (demand_norm(&first), source.clone()));
                    if let Some(s) = source {
                        if !s.is_empty() && s != b {
                            alias_global
                                .entry(b.clone())
                                .or_insert_with(|| (demand_norm(&first), Some(s.clone())));
                        }
                    }
                }
            }
        }
        if target_crates.contains(&demand_norm(&first)) {
            let kind = if u.reexport { "reexport" } else { "use" };
            for (binding, source) in &lv {
                if binding.as_deref() == Some("*") {
                    globs.push(u.path.clone());
                    continue;
                }
                let nm = source
                    .clone()
                    .filter(|s| !s.is_empty())
                    .or_else(|| binding.clone());
                if let Some(nm) = nm {
                    demand.entry(nm).or_default().insert(kind.to_string());
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

    // Coverage: rendered S5 picks (pair keys cover their outer) plus
    // carry keys and carried names.
    let pick_line_re = regex::Regex::new(r"^-\s+`([^`]+)`").expect("static regex compiles");
    let mut pick_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pick_pairs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut in_s5 = false;
    for line in target_orientation.lines() {
        let ls = line.trim_end();
        if ls.starts_with("## 5.") {
            in_s5 = true;
            continue;
        }
        if in_s5 && ls.starts_with("## ") && !ls.starts_with("## 5.") {
            in_s5 = false;
        }
        if !in_s5 {
            continue;
        }
        let Some(cap) = pick_line_re.captures(ls) else {
            continue;
        };
        let pat = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let Some((_, rest)) = pat.split_once(':') else {
            continue;
        };
        if let Some((o, _)) = rest.split_once("::") {
            pick_pairs.insert(rest.to_string());
            if o != "_" {
                pick_names.insert(o.to_string());
            }
        } else {
            pick_names.insert(rest.to_string());
        }
    }
    let mut carry_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(carries) = target_facts.get("carries").and_then(|v| v.as_object()) {
        for (key, entries) in carries {
            if let Some((_, rest)) = key.split_once(':') {
                let outer = rest.split_once("::").map(|(o, _)| o).unwrap_or(rest);
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

/// What: depth-tracked top-level comma split over a flattened
/// use-tree string (commas inside (), [], {}, <> do not split).
///
/// Why: mirrors the python prototype's split_top; deliberately local
/// to the phase-1 module rather than reusing
/// scan::items::helpers::split_top_commas (phase-2 unifies).
fn demand_split_top(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth = (depth - 1).max(0);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.clone());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// What: expand one flattened use-tree string into (binding, source)
/// pairs - binding is the in-scope name, source is the imported
/// item's own name (`X as Y` -> (Y, X-leaf); `{self}` binds the
/// parent segment; `*` marks a glob).
///
/// Why: the consumer's import surface drives both the per-file
/// binding map and demand extraction; mirrors the python prototype's
/// leaves() including its Option-shaped parent handling.
fn demand_leaves(
    s: &str,
    parent_last: Option<&str>,
    out: &mut Vec<(Option<String>, Option<String>)>,
) {
    let s = s.trim();
    if s.is_empty() {
        return;
    }
    if s == "*" {
        out.push((Some("*".to_string()), Some("*".to_string())));
        return;
    }
    if let Some(b) = s.find('{') {
        let prefix = s[..b].trim_end().trim_end_matches(':');
        let prefix_last = if prefix.is_empty() {
            parent_last.map(String::from)
        } else {
            Some(prefix.rsplit("::").next().unwrap_or(prefix).to_string())
        };
        let close = s.rfind('}');
        let inner = match close {
            Some(c) if c > b => &s[b + 1..c],
            _ => &s[b + 1..],
        };
        for piece in demand_split_top(inner) {
            demand_leaves(&piece, prefix_last.as_deref(), out);
        }
        return;
    }
    if let Some((src_path, renamed)) = s.rsplit_once(" as ") {
        let binding = renamed.trim().to_string();
        let src_path = src_path.trim();
        if src_path == "self" {
            out.push((Some(binding), parent_last.map(String::from)));
            return;
        }
        let src_leaf = src_path.rsplit("::").next().unwrap_or(src_path).to_string();
        out.push((Some(binding), Some(src_leaf)));
        return;
    }
    if s == "self" {
        out.push((
            parent_last.map(String::from),
            parent_last.map(String::from),
        ));
        return;
    }
    let leaf = s.rsplit("::").next().unwrap_or(s).trim().to_string();
    out.push((Some(leaf.clone()), Some(leaf)));
}

/// What: affirmative resolution of a consumer site to a target
/// crate: an explicit target-crate qualifier, a per-file import
/// binding to a target crate, or a crate-wide rename whose source is
/// a target crate. Returns the SOURCE name (rename-translated) or
/// None - name-keyed declaration matches alone are NOT inclusion.
fn demand_resolved(
    file: &str,
    name: &str,
    qualifier: Option<&str>,
    imap: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, (String, Option<String>)>,
    >,
    alias_global: &std::collections::HashMap<String, (String, Option<String>)>,
    target_crates: &std::collections::BTreeSet<String>,
) -> Option<String> {
    if let Some(q) = qualifier {
        if target_crates.contains(&demand_norm(q)) {
            if let Some((root, src)) = alias_global.get(name) {
                if target_crates.contains(root) {
                    return src.clone();
                }
            }
            return Some(name.to_string());
        }
    }
    if let Some(hit) = imap.get(file).and_then(|m| m.get(name)) {
        if target_crates.contains(&hit.0) {
            return Some(
                hit.1
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| name.to_string()),
            );
        }
    }
    if let Some((root, src)) = alias_global.get(name) {
        if target_crates.contains(root) {
            return src.clone();
        }
    }
    None
}
