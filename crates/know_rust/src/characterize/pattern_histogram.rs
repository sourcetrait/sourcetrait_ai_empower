use crate::*;

/// What: count candidate-pattern instances across the workspace fact
/// table and return (ranked list, by-kind count map, reg_macro
/// call-site counts).
///
/// Why: characterize.py's `pattern_histogram`. The dominant pattern
/// is decided empirically from this histogram (not assumed to be
/// `impl Trait for`); the by_kind map drives the workspace_shape
/// classifier; reg_calls feeds the registration_macros block in
/// fingerprint.json.
///
/// Where: called from `crate::characterize::run::characterize` once
/// per workspace scan; the output feeds `select_mode` and is written
/// straight into `Fingerprint::pattern_histogram` /
/// `pattern_by_kind` / `registration_macros`.
pub fn pattern_histogram(
    all_facts: &WorkspaceFacts,
    free_fns_by_crate: &indexmap::IndexMap<String, usize>,
) -> (
    Vec<(String, usize)>,
    indexmap::IndexMap<String, usize>,
    indexmap::IndexMap<String, usize>,
) {
    let mut patterns: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let mut by_kind: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let mut reg_macro_calls: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();

    for it in &all_facts.impls {
        if let Some(t) = it.get("trait").and_then(|v| v.as_str()) {
            if !it.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                *patterns.entry(format!("trait_impl:{}", t)).or_default() += 1;
                *by_kind.entry("trait_impl".to_string()).or_default() += 1;
            }
        }
    }
    for d in &all_facts.derives {
        if let Some(t) = d.get("trait").and_then(|v| v.as_str()) {
            *patterns.entry(format!("derive:{}", t)).or_default() += 1;
            *by_kind.entry("derive".to_string()).or_default() += 1;
        }
    }
    for m in &all_facts.macros {
        let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let name = match m.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        if kind == "attr_macro" {
            *patterns.entry(format!("attr_macro:{}", name)).or_default() += 1;
            *by_kind.entry("attr_macro".to_string()).or_default() += 1;
        } else if kind == "macro_invocation" {
            let n_args = m
                .get("arg_idents")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
                .max(1);
            *patterns.entry(format!("reg_macro:{}", name)).or_default() += n_args;
            *by_kind.entry("reg_macro".to_string()).or_default() += n_args;
            *reg_macro_calls.entry(name.to_string()).or_default() += 1;
        }
    }
    for tu in &all_facts.type_usages {
        if let Some(name) = tu.get("name").and_then(|v| v.as_str()) {
            *patterns.entry(format!("type_usage:{}", name)).or_default() += 1;
            *by_kind.entry("type_usage".to_string()).or_default() += 1;
        }
    }
    for (crate_name, n) in free_fns_by_crate {
        if *n >= 20 {
            patterns.insert(format!("fn_table:{}", crate_name), *n);
            *by_kind.entry("fn_table".to_string()).or_default() += n;
        }
    }

    let mut ranked: Vec<(String, usize)> = patterns.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (ranked, by_kind, reg_macro_calls)
}
