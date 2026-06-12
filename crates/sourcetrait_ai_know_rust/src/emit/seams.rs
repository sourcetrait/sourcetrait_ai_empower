use crate::*;

/// What: predicate excluding test / bench / example files from the
/// src/-only architectural pool. Matches both per-crate layouts
/// (`crates/<X>/tests/...`) and workspace-top-level layouts
/// (`tests/...` at workspace root).
///
/// Why: emit.py's `_is_src_file()` (lines 1032-1043). Test-harness
/// crates and example bins are not part of the architectural seam set;
/// without this filter, sourcetrait_empower's process_spawn signal was
/// dominated by `tests/*.rs` Host harnesses rather than the
/// production seam. Also filters the S2 core vocabulary so
/// test-file-only types do not pollute the vocabulary list.
///
/// Where: used by `crate::emit::seams::detected_seams` for the S3
/// seam-spine pool and by `crate::emit::instance::core_vocabulary` for
/// the S2 vocabulary filter.
pub fn is_src_file(file_path: &str) -> bool {
    let excluded = ["tests/", "benches/", "examples/"];
    if excluded.iter().any(|p| file_path.starts_with(p)) {
        return false;
    }
    excluded
        .iter()
        .all(|p| !file_path.contains(&format!("/{}", p)))
}

/// What: build a `{macro_name: sorted [(file, line)] list}` index over
/// `facts['macro_defs']`, deduped by `(name, file, line)` so per-crate
/// roll-ups that count the same definition under multiple aggregation
/// crates collapse cleanly.
///
/// Why: emit.py's `_macro_defs_index()` (lines 1046-1067). The S6
/// UNRESOLVED guardrails downgrade a `macro_rules!` registration entry
/// from "expansion invisible" to "expansion readable inline at <span>"
/// when an inline definition is captured in `facts['macro_defs']`.
/// This index powers that per-macro lookup.
///
/// Where: called by `crate::emit::seams::detected_seams` (for the S3
/// macro-registration count) and by
/// `crate::emit::orientation::render_orientation` (for S6 per-macro
/// entries).
pub fn macro_defs_index(
    facts: &serde_json::Value,
) -> indexmap::IndexMap<String, Vec<(String, i64)>> {
    let mut idx: indexmap::IndexMap<String, Vec<(String, i64)>> = indexmap::IndexMap::new();
    let mut seen: HashSet<(String, String, i64)> = HashSet::new();
    let arr = facts
        .get("macro_defs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for m in &arr {
        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let file = m.get("file").and_then(|v| v.as_str()).unwrap_or("?").to_string();
        let line = m.get("line").and_then(|v| v.as_i64()).unwrap_or(-1);
        let key = (name.to_string(), file.clone(), line);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        idx.entry(name.to_string()).or_default().push((file, line));
    }
    for spans in idx.values_mut() {
        spans.sort();
    }
    idx
}

/// What: a detected seam record - title + agent-facing description +
/// up-to-five seed sites for the agent to anchor the seam discussion
/// at.
///
/// Why: the orientation composer iterates over these to render the S3
/// section; the title labels the seam, the description carries the
/// what-the-static-trace-cannot-see prose, and sites carry the seed
/// references with file + line.
#[derive(Debug, Clone)]
pub struct DetectedSeam {
    pub title: String,
    pub description: String,
    pub sites: Vec<serde_json::Value>,
}

/// What: produce the S3 seam-spine seed list - process / IPC + macro-
/// mediated registration + FFI / syscall - by inspecting the
/// fingerprint's `seam_inventory` + `registration_macros` and the
/// facts's `uses` entries. Spawn-site filtering is src/-only so
/// test-harness sites do not crowd the architectural signal.
///
/// Why: emit.py's `detected_seams()` (lines 1070-1109). Seeds the S3
/// section + the S6 UNRESOLVED guardrails (re-used at the bottom of
/// orientation.md). The macro-mediated-registration description
/// reports how many of the listed macros have inline `macro_rules!`
/// definitions in the workspace, pointing the reader at S6 for
/// per-macro detail.
///
/// Where: called by `crate::emit::orientation::render_orientation`
/// before the section rendering loop starts, and the returned vector
/// is consumed twice (S3 list + S6 list).
pub fn detected_seams(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
) -> Vec<DetectedSeam> {
    let mut seeds: Vec<DetectedSeam> = Vec::new();
    let inv = fp.get("seam_inventory").cloned().unwrap_or(serde_json::Value::Null);
    let has_inv = |key: &str| -> bool {
        inv.get(key)
            .and_then(|v| v.as_u64())
            .map(|n| n > 0)
            .unwrap_or(false)
    };
    if has_inv("process_spawn") || has_inv("std_io_stream") {
        let empty_vec: Vec<serde_json::Value> = Vec::new();
        let uses_arr = facts
            .get("uses")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_vec);
        let spawn_sites: Vec<serde_json::Value> = uses_arr
            .iter()
            .filter(|u| {
                let path = u.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let file = u.get("file").and_then(|v| v.as_str()).unwrap_or("");
                path.contains("process") && is_src_file(file)
            })
            .take(5)
            .cloned()
            .collect();
        seeds.push(DetectedSeam {
            title: "process / IPC boundary".to_string(),
            description: "A child process or stdin/stdout protocol crosses an address-space \
                boundary; static tracing stops here. Verify the wire format in source before \
                authoring across it.".to_string(),
            sites: spawn_sites,
        });
    }
    let reg = fp
        .get("registration_macros")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if !reg.is_empty() {
        let macs_list: Vec<String> = reg.keys().cloned().collect();
        let macs = macs_list.join(", ");
        let defs_idx = macro_defs_index(facts);
        let inline_count = reg.keys().filter(|m| defs_idx.contains_key(*m)).count();
        let total = reg.len();
        let mut desc = String::from(
            "Items are registered by a macro; the call-site argument list is captured but the \
             EXPANSION is not visible to the floor scanner. Counts are unverified without the \
             rustdoc overlay.",
        );
        if inline_count > 0 {
            desc.push_str(&format!(
                " {} of {} macros have inline `macro_rules!` definitions in the workspace - see \
                 S6 for per-macro detail with definition spans (expansion is statically \
                 followable for those).",
                inline_count, total
            ));
        } else {
            desc.push_str(
                " Confirm the generated items in source or via overlay before relying on the \
                 registry.",
            );
        }
        seeds.push(DetectedSeam {
            title: format!("macro-mediated registration ({})", macs),
            description: desc,
            sites: Vec::new(),
        });
    }
    if has_inv("extern") || has_inv("syscall_libc") {
        seeds.push(DetectedSeam {
            title: "FFI / syscall boundary".to_string(),
            description: "`extern`/`libc` crosses into non-Rust or the kernel; static tracing \
                stops at the boundary. Verify the foreign contract before authoring.".to_string(),
            sites: Vec::new(),
        });
    }
    seeds
}
