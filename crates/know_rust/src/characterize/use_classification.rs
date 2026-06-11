use crate::*;

/// What: 4-bucket use-classification (end_use / dev_use /
/// end_with_dev_use / dev_with_end_use) per crate, then aggregate
/// the workspace-level label + bucket counts + scaffolding crate
/// list + human-readable reasoning.
///
/// Why: characterize.py's `_classify_workspace_use`. Downstream
/// emit's prompt selection depends on this label; the per-crate
/// breakdown surfaces in fingerprint.json for auditability.
///
/// Where: called from `crate::characterize::run::characterize` after
/// pattern_metrics is computed (the per-crate flip from
/// end_with_dev_use to dev_with_end_use depends on the cross-crate
/// is_pub usage signal).
pub fn classify_workspace_use(
    crates: &indexmap::IndexMap<String, CrateInfo>,
    pattern_metrics: &indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
) -> WorkspaceUseClassification {
    let mut per_crate: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
    let mut scaffolding: Vec<String> = Vec::new();
    for (name, info) in crates {
        if is_scaffolding(name, info) {
            scaffolding.push(name.clone());
            continue;
        }
        per_crate.insert(
            name.clone(),
            classify_crate_use(name, info, pattern_metrics, calibration),
        );
    }

    let mut buckets: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    buckets.insert("end_use".to_string(), 0);
    buckets.insert("dev_use".to_string(), 0);
    buckets.insert("end_with_dev_use".to_string(), 0);
    buckets.insert("dev_with_end_use".to_string(), 0);
    for cls in per_crate.values() {
        if let Some(c) = buckets.get_mut(cls) {
            *c += 1;
        }
    }
    let n_total: usize = buckets.values().sum();
    let n_scaf = scaffolding.len();

    let (workspace, mut reasoning) = if n_total == 0 {
        (
            "dev_use".to_string(),
            "no primary crates after scaffolding exclusion".to_string(),
        )
    } else if buckets["dev_use"] == n_total {
        (
            "dev_use".to_string(),
            format!("all {} primary crates are pure dev_use libraries", n_total),
        )
    } else if buckets["end_use"] == n_total {
        (
            "end_use".to_string(),
            format!("all {} primary crates are pure end_use binaries", n_total),
        )
    } else if buckets["end_with_dev_use"] > 0 || buckets["end_use"] > 0 {
        (
            "end_with_dev_use".to_string(),
            format!(
                "primary crates: end_use={} end_with_dev_use={} dev_with_end_use={} dev_use={}; the workspace ships an end-user product with the libraries composing it",
                buckets["end_use"],
                buckets["end_with_dev_use"],
                buckets["dev_with_end_use"],
                buckets["dev_use"],
            ),
        )
    } else if buckets["dev_with_end_use"] > 0 {
        (
            "dev_with_end_use".to_string(),
            format!(
                "primary crates: dev_use={} dev_with_end_use={}; the workspace's primary deliverable is a library with a CLI auxiliary (gitoxide pattern)",
                buckets["dev_use"], buckets["dev_with_end_use"],
            ),
        )
    } else {
        ("dev_use".to_string(), "fallback default".to_string())
    };
    if n_scaf > 0 {
        reasoning.push_str(&format!(" ({} scaffolding crates excluded)", n_scaf));
    }
    scaffolding.sort();
    WorkspaceUseClassification {
        workspace,
        per_crate,
        buckets,
        scaffolding_crates: scaffolding,
        reasoning,
    }
}

fn classify_crate_use(
    name: &str,
    info: &CrateInfo,
    pattern_metrics: &indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
) -> String {
    let has_bin = info.has_bin;
    let has_lib = info.has_lib;
    if has_lib && !has_bin {
        return "dev_use".to_string();
    }
    if has_bin && !has_lib {
        return "end_use".to_string();
    }
    if !has_bin && !has_lib {
        return "dev_use".to_string();
    }
    let pub_inter_count: usize = pattern_metrics
        .values()
        .filter(|m| m.is_pub && m.defining_crate.as_deref() == Some(name))
        .map(|m| m.inter_count)
        .sum();
    if pub_inter_count >= calibration.classification.dev_with_end_threshold {
        "dev_with_end_use".to_string()
    } else {
        "end_with_dev_use".to_string()
    }
}

pub(crate) fn is_scaffolding(name: &str, info: &CrateInfo) -> bool {
    let d = info.dir.to_lowercase();
    let path_segments = [
        "/examples/", "examples/", "/example/", "example/",
        "/benches/", "benches/", "/bench/", "bench/",
        "/fuzz/", "fuzz/",
        "/tests/", "tests/", "/test/", "test/",
        "/xtask/", "xtask/",
        "/tools/", "tools/",
        "/ci/", "ci/",
        "/scripts/", "scripts/",
        "/build/", "build/",
    ];
    if path_segments.iter().any(|seg| d.contains(seg)) {
        return true;
    }
    let bare_dirs = [
        "xtask", "ci", "bogo", "build", "tools", "tests",
        "scripts", "examples", "benches", "bench", "fuzz",
    ];
    if bare_dirs.iter().any(|b| d == *b) {
        return true;
    }
    let nm = name.to_lowercase();
    let suffix_patterns = [
        "_example", "_demo", "_fuzz", "_test", "_tests",
        "_bench", "_benches", "_xtask",
        "-example", "-demo", "-fuzz", "-test", "-tests",
        "-bench", "-benches", "-xtask",
    ];
    if suffix_patterns.iter().any(|s| nm.contains(s)) {
        return true;
    }
    let prefix_patterns = [
        "example_", "example-", "demo_", "demo-",
        "build_", "build-", "build_templated", "ci_", "ci-",
        "tests-", "test-", "bench-", "bench_",
        "fuzz_", "fuzz-",
        "rustls-bench", "rustls-ci-bench",
        "export-content", "export_content",
        "tests-integration",
    ];
    if prefix_patterns.iter().any(|p| nm.starts_with(p)) {
        return true;
    }
    let bare_names = [
        "xtask", "ci", "bogo", "build", "tools",
        "example-showcase", "export-content", "tests-integration",
    ];
    bare_names.iter().any(|b| nm == *b)
}
