use crate::*;

/// What: render the exhaustive `reference.md` per-crate index from a
/// characterize fingerprint + facts pair. Groups types / traits /
/// impls / free fns / macros / reexports by crate and emits sorted
/// bullet entries with `file:line` span anchors.
///
/// Why: emit.py's `emit_reference` (lines 332-391). The reference is
/// grep-into, not read-top-to-bottom. Provides the agent with span
/// anchors for every workspace-defined item; null-span items
/// (re-exports, blanket/synthesized/macro-generated impls per rustdoc
/// overlay) are flagged with annotations, not dropped.
///
/// Where: called from `crate::emit::run::emit` after fingerprint +
/// facts are loaded.
pub fn render_reference(
    workspace_root: &Path,
    out_dir: &Path,
    fp: &serde_json::Value,
    facts: &serde_json::Value,
) -> String {
    let mut by_crate: indexmap::IndexMap<String, CrateBucket> = indexmap::IndexMap::new();

    if let Some(arr) = facts.get("types").and_then(|v| v.as_array()) {
        for t in arr {
            let crate_name = t.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            by_crate.entry(crate_name).or_default().types.push(t.clone());
        }
    }
    if let Some(arr) = facts.get("traits").and_then(|v| v.as_array()) {
        for t in arr {
            let crate_name = t.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            by_crate.entry(crate_name).or_default().traits.push(t.clone());
        }
    }
    if let Some(arr) = facts.get("impls").and_then(|v| v.as_array()) {
        for i in arr {
            let crate_name = i.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            by_crate.entry(crate_name).or_default().impls.push(i.clone());
        }
    }
    if let Some(arr) = facts.get("fns").and_then(|v| v.as_array()) {
        for f in arr {
            let brace_depth = f.get("brace_depth").and_then(|v| v.as_i64()).unwrap_or(99);
            if brace_depth == 0 {
                let crate_name = f.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                by_crate.entry(crate_name).or_default().fns.push(f.clone());
            }
        }
    }
    if let Some(arr) = facts.get("macros").and_then(|v| v.as_array()) {
        for m in arr {
            let crate_name = m.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            by_crate.entry(crate_name).or_default().macros.push(m.clone());
        }
    }
    if let Some(arr) = facts.get("uses").and_then(|v| v.as_array()) {
        for u in arr {
            let reexport = u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false);
            if reexport {
                let crate_name = u.get("crate").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                by_crate.entry(crate_name).or_default().reexports.push(u.clone());
            }
        }
    }

    let mut crate_names: Vec<String> = by_crate.keys().cloned().collect();
    crate_names.sort();

    let mut lines: Vec<String> = Vec::new();
    lines.push("# Reference Index".to_string());
    lines.push(String::new());
    lines.push("Exhaustive, span-anchored. Grep this; do not read it top to bottom. Every entry is".to_string());
    lines.push("a `file:line` you open in source to verify or extend. Spans the rustdoc overlay".to_string());
    lines.push("marks null (re-exports, blanket/synthesized/macro-generated impls) are flagged, not".to_string());
    lines.push("dropped.".to_string());
    lines.push(String::new());
    lines.push("```".to_string());
    lines.push(provenance(workspace_root, out_dir, fp));
    lines.push("```".to_string());
    lines.push(String::new());

    for crate_name in crate_names {
        let bucket = by_crate.get(&crate_name).cloned().unwrap_or_default();
        lines.push(format!("## crate: {}", crate_name));
        if !bucket.traits.is_empty() {
            lines.push("### traits".to_string());
            let mut sorted = bucket.traits.clone();
            sorted.sort_by(|a, b| {
                let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                na.cmp(nb)
            });
            for t in sorted {
                let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let cfg = if t.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "  *(cfg-gated)*"
                } else {
                    ""
                };
                lines.push(format!("- `{}` - {}{}", name, span(&t), cfg));
            }
        }
        if !bucket.types.is_empty() {
            lines.push("### types".to_string());
            let mut sorted = bucket.types.clone();
            sorted.sort_by(|a, b| {
                let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                na.cmp(nb)
            });
            for t in sorted {
                let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let cfg = if t.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "  *(cfg-gated)*"
                } else {
                    ""
                };
                lines.push(format!("- `{} {}` - {}{}", kind, name, span(&t), cfg));
            }
        }
        if !bucket.impls.is_empty() {
            lines.push("### impls".to_string());
            let mut sorted = bucket.impls.clone();
            sorted.sort_by(|a, b| {
                let ta = a.get("trait").and_then(|v| v.as_str()).map(String::from).unwrap_or("None".to_string());
                let tb = b.get("trait").and_then(|v| v.as_str()).map(String::from).unwrap_or("None".to_string());
                let tya = a.get("type").and_then(|v| v.as_str()).map(String::from).unwrap_or("None".to_string());
                let tyb = b.get("type").and_then(|v| v.as_str()).map(String::from).unwrap_or("None".to_string());
                ta.cmp(&tb).then(tya.cmp(&tyb))
            });
            for i in sorted {
                let trait_str = match i.get("trait").and_then(|v| v.as_str()) {
                    Some(t) => format!("`{}` for ", t),
                    None => "(inherent) ".to_string(),
                };
                let type_str = i.get("type").and_then(|v| v.as_str()).unwrap_or("None");
                let cfg = if i.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "  *(cfg-gated)*"
                } else {
                    ""
                };
                lines.push(format!(
                    "- impl {}`{}` - {}{}",
                    trait_str,
                    type_str,
                    span(&i),
                    cfg
                ));
            }
        }
        if !bucket.fns.is_empty() {
            lines.push("### free functions".to_string());
            let mut sorted = bucket.fns.clone();
            sorted.sort_by(|a, b| {
                let na = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nb = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                na.cmp(nb)
            });
            for f in sorted {
                let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("- `fn {}` - {}", name, span(&f)));
            }
        }
        if !bucket.macros.is_empty() {
            lines.push(
                "### macro applications *(expansion unverified without rustdoc overlay)*".to_string(),
            );
            let mut sorted = bucket.macros.clone();
            sorted.sort_by(|a, b| {
                let la = a.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
                let lb = b.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
                la.cmp(&lb)
            });
            for m in sorted {
                let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if kind == "macro_invocation" {
                    let args_arr = m
                        .get("arg_idents")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let args_strs: Vec<String> = args_arr
                        .iter()
                        .take(12)
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    let args = args_strs.join(", ");
                    lines.push(format!("- `{}!(...)` args=[{}] - {}", name, args, span(&m)));
                } else {
                    lines.push(format!("- `#[{}]` - {}", name, span(&m)));
                }
            }
        }
        if !bucket.reexports.is_empty() {
            lines.push(
                "### re-exports *(rustdoc resolves the target; span may be null)*".to_string(),
            );
            for u in bucket.reexports {
                let path = u.get("path").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("- `{}` - {}", path, span(&u)));
            }
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

#[derive(Default, Clone)]
struct CrateBucket {
    types: Vec<serde_json::Value>,
    traits: Vec<serde_json::Value>,
    impls: Vec<serde_json::Value>,
    fns: Vec<serde_json::Value>,
    macros: Vec<serde_json::Value>,
    reexports: Vec<serde_json::Value>,
}
