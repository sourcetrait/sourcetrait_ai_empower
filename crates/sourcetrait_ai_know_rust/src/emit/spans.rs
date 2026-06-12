use crate::*;

/// What: format a record's span as `file:line` or `file:line-end_line`
/// when the record carries a multi-line `end_line` field, falling back
/// to `?` for missing components. Takes a `serde_json::Value`
/// representing a fact entry.
///
/// Why: emit.py's `sp()` (lines 306-311). Every span anchor in
/// orientation.md + reference.md goes through this helper so the
/// agent's grep-target format is consistent across sections.
///
/// Where: used by `crate::emit::reference::render_reference` for the
/// per-item bullets and by the orientation composer + instance picker
/// for seed instance spans throughout S2-S6.
pub fn span(rec: &serde_json::Value) -> String {
    let file = rec.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = rec.get("line").and_then(|v| v.as_i64()).unwrap_or(-1);
    let line_str = if line < 0 {
        "?".to_string()
    } else {
        line.to_string()
    };
    let end = rec.get("end_line").and_then(|v| v.as_i64());
    match end {
        Some(e) if e != line => format!("{}:{}-{}", file, line_str, e),
        _ => format!("{}:{}", file, line_str),
    }
}

/// What: build the provenance banner string (commit + rustc +
/// tool_version + rustdoc_overlay_present) used inside the
/// ```triple-backtick block at the top of both reference.md and
/// orientation.md.
///
/// Why: emit.py's `provenance()` (lines 314-329). Records the git
/// commit + rustc + tool version + whether the rustdoc overlay was
/// available, so the agent can correlate orientation content with the
/// exact workspace + toolchain state. Required at the top of every
/// emitted artifact.
///
/// Where: called by `crate::emit::reference::render_reference` and the
/// orientation + container_routing composers before the per-section
/// rendering begins.
pub fn provenance(root: &Path, out_dir: &Path, fp: &serde_json::Value) -> String {
    let commit = process::Command::new("git")
        .args(["-C", &root.to_string_lossy(), "rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let rustc = process::Command::new("rustc")
        .args(["--version"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let tool_version = fp
        .get("tool_version")
        .and_then(|v| v.as_str())
        .unwrap_or("None");
    let overlay = if out_dir.join("rustdoc_overlay.json").exists() {
        "True"
    } else {
        "False"
    };
    format!(
        "commit: {}\nrustc: {}\ntool_version: {}\nrustdoc_overlay_present: {}",
        commit, rustc, tool_version, overlay
    )
}
