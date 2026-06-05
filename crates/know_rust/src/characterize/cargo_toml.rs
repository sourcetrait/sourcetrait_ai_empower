use crate::*;

/// What: enumerate workspace crates via `cargo metadata --no-deps`
/// (cargo is the authoritative source of workspace structure +
/// iteration order). Returns (a) the per-crate metadata map in
/// cargo's package order and (b) the list of workspace-root
/// directories. Falls back to a per-subdir cargo metadata walk for
/// submodule-aggregator repos (cosmic-epoch) that have no root
/// Cargo.toml.
///
/// Why: aligns the crate enumeration with cargo's authoritative
/// view. The_user 2026-06-05: cargo is the canonical source-of-truth
/// for workspace ordering; rglob/walkdir produces host-dependent
/// orders that vary between Python and Rust + between filesystems.
/// cargo metadata's order is portable and matches what cargo itself
/// resolves to. The fallback path handles the submodule-aggregator
/// case while staying within the cargo-metadata method.
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

    if let Some(meta) = try_cargo_metadata(root) {
        add_packages(&mut crates, &mut workspace_roots, root, &meta);
    } else {
        let mut subs: Vec<PathBuf> = match fs::read_dir(root) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter(|p| p.join("Cargo.toml").is_file())
                .collect(),
            Err(_) => Vec::new(),
        };
        subs.sort();
        for sub in subs {
            if let Some(meta) = try_cargo_metadata(&sub) {
                add_packages(&mut crates, &mut workspace_roots, root, &meta);
            }
        }
    }

    (crates, workspace_roots.into_iter().collect())
}

fn try_cargo_metadata(working_dir: &Path) -> Option<serde_json::Value> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(working_dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
}

fn add_packages(
    crates: &mut indexmap::IndexMap<String, CrateInfo>,
    workspace_roots: &mut std::collections::BTreeSet<String>,
    root: &Path,
    meta: &serde_json::Value,
) {
    let ws_root_str = meta
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let ws_root = if ws_root_str.is_empty() {
        root.to_path_buf()
    } else {
        PathBuf::from(ws_root_str)
    };
    let ws_rel = match ws_root.strip_prefix(root) {
        Ok(r) => {
            let s = r.to_string_lossy().to_string();
            if s.is_empty() {
                ".".to_string()
            } else {
                s
            }
        }
        Err(_) => ".".to_string(),
    };
    workspace_roots.insert(ws_rel);

    let packages = meta
        .get("packages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for pkg in packages {
        let name = match pkg.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let manifest_path = pkg
            .get("manifest_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if manifest_path.is_empty() {
            continue;
        }
        let crate_dir = PathBuf::from(manifest_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let dir_rel = match crate_dir.strip_prefix(root) {
            Ok(r) => {
                let s = r.to_string_lossy().to_string();
                if s.is_empty() {
                    ".".to_string()
                } else {
                    s
                }
            }
            Err(_) => continue,
        };
        let mut deps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        if let Some(deps_arr) = pkg.get("dependencies").and_then(|v| v.as_array()) {
            for d in deps_arr {
                if let Some(n) = d.get("name").and_then(|v| v.as_str()) {
                    deps.insert(n.to_string());
                }
            }
        }
        let targets = pkg
            .get("targets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let has_bin_entry = targets.iter().any(|t| {
            t.get("kind")
                .and_then(|k| k.as_array())
                .map(|arr| arr.iter().any(|x| x.as_str() == Some("bin")))
                .unwrap_or(false)
        });
        let has_main_rs = crate_dir.join("src").join("main.rs").exists();
        let has_bin_dir = crate_dir.join("src").join("bin").is_dir();
        let has_bin = has_bin_entry || has_main_rs || has_bin_dir;
        let has_lib_entry = targets.iter().any(|t| {
            t.get("kind")
                .and_then(|k| k.as_array())
                .map(|arr| {
                    arr.iter()
                        .any(|x| x.as_str() == Some("lib") || x.as_str() == Some("proc-macro"))
                })
                .unwrap_or(false)
        });
        let has_lib_rs = crate_dir.join("src").join("lib.rs").exists();
        let has_lib = has_lib_entry || has_lib_rs;
        let keywords: Vec<String> = pkg
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let categories: Vec<String> = pkg
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let description = pkg
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        crates.insert(
            name,
            CrateInfo {
                dir: dir_rel,
                deps: deps.into_iter().collect(),
                has_bin,
                has_lib,
                keywords,
                categories,
                description,
            },
        );
    }
}
