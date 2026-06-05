use crate::*;

/// What: walk a workspace root for every `Cargo.toml`, parse each via
/// the `toml` crate, and return (a) the per-crate metadata map keyed
/// by package name and (b) the sorted list of workspace-root
/// directories discovered.
///
/// Why: equivalent to characterize.py's `find_crates`. Captures the
/// bin / lib presence signals (the_user 2026-06-03), the dependency
/// graph (for components + use-classification), and the package
/// metadata downstream emit may surface. The python output uses
/// insertion order; `IndexMap` preserves the walk order so the
/// downstream fingerprint matches.
///
/// Where: called from `crate::characterize::run::characterize` after
/// resolving the workspace root.
pub fn find_crates(
    root: &Path,
) -> (
    indexmap::IndexMap<String, CrateInfo>,
    Vec<String>,
) {
    let mut crates: indexmap::IndexMap<String, CrateInfo> = indexmap::IndexMap::new();
    let mut workspace_roots: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();

    for entry in walkdir::WalkDir::new(root) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.file_name().and_then(|s| s.to_str()) != Some("Cargo.toml") {
            continue;
        }
        if path.components().any(|c| c.as_os_str() == "target") {
            continue;
        }
        let text = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: toml::Value = match toml::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let cargo_dir = path.parent().unwrap_or(root);
        let rel_dir = match cargo_dir.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if parsed.get("workspace").is_some() {
            workspace_roots.insert(if rel_dir.is_empty() { ".".to_string() } else { rel_dir.clone() });
        }

        let Some(pkg) = parsed.get("package").and_then(|v| v.as_table()) else {
            continue;
        };
        let Some(name) = pkg.get("name").and_then(|v| v.as_str()) else {
            continue;
        };

        let mut deps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for sect in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(d) = parsed.get(*sect).and_then(|v| v.as_table()) {
                for key in d.keys() {
                    deps.insert(key.clone());
                }
            }
        }

        let has_bin_entry = parsed.get("bin").map(|v| !v.as_array().map(|a| a.is_empty()).unwrap_or(true)).unwrap_or(false);
        let has_main_rs = cargo_dir.join("src").join("main.rs").exists();
        let has_bin_dir = cargo_dir.join("src").join("bin").is_dir();
        let has_bin = has_bin_entry || has_main_rs || has_bin_dir;

        let has_lib_entry = parsed.get("lib").is_some();
        let has_lib_rs = cargo_dir.join("src").join("lib.rs").exists();
        let has_lib = has_lib_entry || has_lib_rs;

        let keywords = pkg
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let categories = pkg
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let description = pkg
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        crates.insert(
            name.to_string(),
            CrateInfo {
                dir: if rel_dir.is_empty() { ".".to_string() } else { rel_dir },
                deps: deps.into_iter().collect(),
                has_bin,
                has_lib,
                keywords,
                categories,
                description,
            },
        );
    }

    (crates, workspace_roots.into_iter().collect())
}
