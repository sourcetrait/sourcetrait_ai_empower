use crate::*;

/// What: render the routing-doc orientation shape for workspaces
/// classified as containers by the shape heuristic. Returns the
/// `orientation.md` content as a single string with provenance + how-
/// shaped + workspace members listing + agent routing prompt +
/// histogram appendix.
///
/// Why: emit.py's `emit_container_routing()` (lines 1112-1184). When
/// `workspace_shape.shape == container` (sourcetrait_common as the
/// canonical case), the workspace has no single architectural pattern
/// and running the picker across it as a whole would produce
/// confidently-wrong output. This composer routes the agent to the
/// per-member sub-orientations instead.
///
/// Where: dispatched by `crate::emit::run::emit` when the fingerprint's
/// `workspace_shape.shape` field equals "container"; the standard
/// `render_orientation` handles all other shapes.
pub fn render_container_routing(
    workspace_root: &Path,
    out_dir: &Path,
    fp: &serde_json::Value,
) -> String {
    let shape = fp
        .get("workspace_shape")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let signals = shape
        .get("signals")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let mut lines: Vec<String> = Vec::new();
    lines.push("# Orientation: Container Workspace".to_string());
    lines.push(String::new());
    lines.push(
        "The structural signals indicate this workspace is a container - a sub-topic aggregator \
         with no single architectural pattern. Each member is a separate topical library; \
         running the picker across the workspace as a whole would produce confidently-wrong \
         output. This artifact routes you to the per-member orientations instead.".to_string(),
    );
    lines.push(String::new());
    lines.push("```".to_string());
    lines.push(provenance(workspace_root, out_dir, fp));
    lines.push("```".to_string());
    lines.push(String::new());

    lines.push("## How this artifact was shaped".to_string());
    lines.push(String::new());
    lines.push("- shape: **container** (picker bypassed)".to_string());
    lines.push(format!(
        "- reasoning: {}",
        shape.get("reasoning").and_then(|v| v.as_str()).unwrap_or("?")
    ));
    let central_crate = signals
        .get("central_crate")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "?".to_string());
    let central_kind = signals
        .get("central_kind")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "?".to_string());
    lines.push(format!(
        "- central_crate: `{}` (dominant kind: {})",
        central_crate, central_kind
    ));
    lines.push(format!(
        "- uniqueness_ratio: {} (higher -> patterns more crate-isolated)",
        signal_str(&signals, "uniqueness_ratio")
    ));
    lines.push(format!(
        "- kind_dominance_dispersion: {} (higher -> crates have diverse dominant kinds)",
        signal_str(&signals, "kind_dominance_dispersion")
    ));
    lines.push(format!("- leaf_ratio: {}", signal_str(&signals, "leaf_ratio")));
    lines.push(format!("- hub_centrality: {}", signal_str(&signals, "hub_centrality")));
    lines.push(String::new());

    lines.push("## Workspace members".to_string());
    lines.push(String::new());
    lines.push(
        "Each member is a separate topical library. To produce an architectural orientation for \
         a specific topic, run know_rust against the sub-workspace of interest.".to_string(),
    );
    lines.push(String::new());
    let per_crate = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut crate_names: Vec<String> = per_crate.keys().cloned().collect();
    crate_names.sort();
    for name in &crate_names {
        let c = match per_crate.get(name) {
            Some(c) => c,
            None => continue,
        };
        let dir = c.get("dir").and_then(|v| v.as_str()).unwrap_or("?");
        let sloc = c.get("sloc").and_then(|v| v.as_i64()).unwrap_or(0);
        let n_impls = c.get("n_impls").and_then(|v| v.as_i64()).unwrap_or(0);
        let n_types = c.get("n_types").and_then(|v| v.as_i64()).unwrap_or(0);
        let deps_arr: Vec<String> = c
            .get("deps")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|d| d.as_str().map(String::from))
                    .filter(|d| per_crate.contains_key(d))
                    .collect()
            })
            .unwrap_or_default();
        let dep_str = if deps_arr.is_empty() {
            String::new()
        } else {
            format!(" -> depends on: {}", deps_arr.join(", "))
        };
        lines.push(format!(
            "- **{}** ({}/, {} SLOC, {} impls, {} types){}",
            name, dir, sloc, n_impls, n_types, dep_str
        ));
    }
    lines.push(String::new());
    lines.push(
        "**[AGENT]** Pick the member whose architectural pattern you want to trace, then run \
         know_rust against that member's directory. Open the corresponding orientation.md for \
         the per-topic worked slices and authoring guides. The container workspace itself has \
         no unifying architectural pattern to trace; do not author across member boundaries \
         without first reading each member's individual orientation.".to_string(),
    );
    lines.push(String::new());

    lines.push("## Appendix: full pattern histogram".to_string());
    lines.push(String::new());
    lines.push(
        "Reported for auditing the container annotation. If the histogram surfaces a single \
         dominant architectural pattern that you believe IS the workspace's central pattern, \
         consider removing the `container = true` annotation. Otherwise the flat distribution \
         typical of containers should be visible here.".to_string(),
    );
    lines.push(String::new());
    let histogram = fp
        .get("pattern_histogram")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for row in histogram.iter().take(25) {
        let pattern = row.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let count = row.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        lines.push(format!("- `{}` - {}", pattern, count));
    }
    lines.push(String::new());

    lines.join("\n")
}

/// What: render a signal value from the workspace_shape.signals dict
/// as Python would format it - float / int directly via `{}`, missing
/// as `?`, falsey-but-present values still printed. Used in the
/// how-shaped section of the container routing doc.
///
/// Why: Python f-string defaults vary by type; this helper centralises
/// the "render or ?" choice so the container-routing emit stays byte-
/// equal to Python.
///
/// Where: internal helper for `render_container_routing`.
fn signal_str(signals: &serde_json::Value, key: &str) -> String {
    match signals.get(key) {
        Some(v) if v.is_null() => "?".to_string(),
        Some(v) if v.is_string() => v.as_str().unwrap_or("?").to_string(),
        Some(v) if v.is_i64() => v.as_i64().unwrap().to_string(),
        Some(v) if v.is_u64() => v.as_u64().unwrap().to_string(),
        Some(v) if v.is_f64() => format_float_python(v.as_f64().unwrap()),
        Some(v) if v.is_boolean() => {
            if v.as_bool().unwrap() {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Some(v) => v.to_string(),
        None => "?".to_string(),
    }
}

/// What: format an f64 the way Python's default `f"{x}"` would. Integer
/// floats render as `7.0`; other floats use Python's default repr.
///
/// Why: signals like uniqueness_ratio + kind_dominance_dispersion come
/// as floats from the characterize output; Python f-strings produce
/// stable representations that need byte-equal mirroring.
///
/// Where: internal helper for `signal_str`.
fn format_float_python(x: f64) -> String {
    if x.is_nan() {
        "nan".to_string()
    } else if x.is_infinite() {
        if x > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else if x == x.trunc() && x.abs() < 1e16 {
        format!("{}.0", x as i64)
    } else {
        format!("{}", x)
    }
}
