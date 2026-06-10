use crate::*;

/// What: discover the repo's workspace structure via `cargo metadata
/// --no-deps`: the host workspace (when a root manifest exists, else
/// the per-subdir fallback for aggregator repos), plus every EMBEDDED
/// workspace unit reached through an in-repo path-dep whose directory
/// is not itself a member (libcosmic's `./iced` fork). Returns the
/// per-package crate map (each `CrateInfo` carrying version /
/// lib-rename / dep-renames / unit tag), the workspace roots, and the
/// unit table with provenance (host / submodule url+gitlink rev /
/// in-repo). A declared-but-unpopulated unit (empty submodule
/// placeholder) is recorded with full identity and no members.
///
/// Why: identity = provenance, names = bindings (the_user ruling
/// 2026-06-10). cargo is the authoritative source of workspace
/// structure and iteration order; embedded workspaces are modeled as
/// units - never flattened into host membership - so a vendored fork
/// is a different thing from any same-named project, and per-crate
/// attribution lands on the unit's real crates.
///
/// Where: called from `crate::characterize::run::characterize` after
/// resolving the workspace root.
pub(crate) fn find_crates(root: &Path) -> WorkspaceDiscovery {
    let mut discovery = WorkspaceDiscovery::default();
    let mut workspace_roots: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut queue: Vec<PathBuf> = Vec::new();
    let gitmodules = parse_gitmodules(root);

    if let Some(meta) = try_cargo_metadata(root) {
        let ws_rel = meta_ws_rel(root, &meta);
        let (members, path_deps) = ingest_metadata(
            root,
            &meta,
            &ws_rel,
            &mut discovery.crates,
            &mut workspace_roots,
        );
        discovery.units.insert(
            ws_rel.clone(),
            WorkspaceUnit {
                root_dir: ws_rel,
                provenance: UnitProvenance::Host,
                members,
                populated: true,
            },
        );
        queue.extend(path_deps);
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
                let ws_rel = meta_ws_rel(root, &meta);
                if discovery.units.contains_key(&ws_rel) {
                    continue;
                }
                let (members, path_deps) = ingest_metadata(
                    root,
                    &meta,
                    &ws_rel,
                    &mut discovery.crates,
                    &mut workspace_roots,
                );
                let provenance = provenance_for(root, &ws_rel, &gitmodules);
                discovery.units.insert(
                    ws_rel.clone(),
                    WorkspaceUnit {
                        root_dir: ws_rel,
                        provenance,
                        members,
                        populated: true,
                    },
                );
                queue.extend(path_deps);
            }
        }
    }

    // Embedded-unit walk: in-repo path-dep dirs that are not member
    // dirs are unit candidates. Populated candidates ingest their own
    // workspace metadata; unpopulated ones (missing manifest - the
    // empty-submodule placeholder) record identity only.
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(dep_dir) = queue.pop() {
        let rel = match dep_dir.strip_prefix(root) {
            Ok(r) => {
                let s = r.to_string_lossy().to_string();
                if s.is_empty() { ".".to_string() } else { s }
            }
            // Outside the repo: not in-repo source, not a unit.
            Err(_) => continue,
        };
        if !visited.insert(rel.clone()) {
            continue;
        }
        if discovery.units.contains_key(&rel) {
            continue;
        }
        // A path-dep pointing AT an already-ingested member crate is
        // an ordinary sibling dependency, not an embedded unit.
        if discovery.crates.values().any(|c| c.dir == rel) {
            continue;
        }
        if dep_dir.join("Cargo.toml").is_file() {
            if let Some(meta) = try_cargo_metadata(&dep_dir) {
                let ws_rel = meta_ws_rel(root, &meta);
                if discovery.units.contains_key(&ws_rel) {
                    continue;
                }
                let (members, path_deps) = ingest_metadata(
                    root,
                    &meta,
                    &ws_rel,
                    &mut discovery.crates,
                    &mut workspace_roots,
                );
                let provenance = provenance_for(root, &ws_rel, &gitmodules);
                discovery.units.insert(
                    ws_rel.clone(),
                    WorkspaceUnit {
                        root_dir: ws_rel,
                        provenance,
                        members,
                        populated: true,
                    },
                );
                queue.extend(path_deps);
                continue;
            }
        }
        // Unpopulated candidate: coalesce to the SUBMODULE root when
        // one declares this path - ten path-deps into ./iced/<sub>
        // are one embedded unit (the fork workspace), not ten. With
        // no submodule anchor the dep dir itself is the only
        // structural root signal.
        let (unit_rel, provenance) = match submodule_for(&rel, &gitmodules) {
            Some((sub_path, url)) => (
                sub_path.clone(),
                UnitProvenance::Submodule {
                    url: url.clone(),
                    rev: gitlink_rev(root, &sub_path),
                },
            ),
            None => (rel.clone(), UnitProvenance::InRepo),
        };
        if discovery.units.contains_key(&unit_rel) {
            continue;
        }
        discovery.units.insert(
            unit_rel.clone(),
            WorkspaceUnit {
                root_dir: unit_rel,
                provenance,
                members: Vec::new(),
                populated: false,
            },
        );
    }

    discovery.workspace_roots = workspace_roots.into_iter().collect();
    discovery
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

/// What: the repo-relative workspace root of a cargo metadata blob
/// (`.` when it equals the repo root or cannot be related to it).
fn meta_ws_rel(root: &Path, meta: &serde_json::Value) -> String {
    let ws_root_str = meta
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let ws_root = if ws_root_str.is_empty() {
        root.to_path_buf()
    } else {
        PathBuf::from(ws_root_str)
    };
    match ws_root.strip_prefix(root) {
        Ok(r) => {
            let s = r.to_string_lossy().to_string();
            if s.is_empty() { ".".to_string() } else { s }
        }
        Err(_) => ".".to_string(),
    }
}

/// What: ingest one cargo metadata blob's packages into the crate
/// map under `unit_key`, recording the unit root in workspace_roots.
/// Returns (member package names, absolute in-repo path-dep dirs for
/// the embedded-unit walk). Package-name collisions across units keep
/// the first ingest and warn (the name keys are bindings; identity
/// disambiguation beyond first-wins is future model work).
fn ingest_metadata(
    root: &Path,
    meta: &serde_json::Value,
    unit_key: &str,
    crates: &mut indexmap::IndexMap<String, CrateInfo>,
    workspace_roots: &mut std::collections::BTreeSet<String>,
) -> (Vec<String>, Vec<PathBuf>) {
    workspace_roots.insert(unit_key.to_string());
    let mut members: Vec<String> = Vec::new();
    let mut path_deps: Vec<PathBuf> = Vec::new();

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
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut deps: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut renames: Vec<(String, String)> = Vec::new();
        if let Some(deps_arr) = pkg.get("dependencies").and_then(|v| v.as_array()) {
            for d in deps_arr {
                let dep_name = match d.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n,
                    None => continue,
                };
                deps.insert(dep_name.to_string());
                if let Some(rename) = d.get("rename").and_then(|v| v.as_str()) {
                    renames.push((rename.to_string(), dep_name.to_string()));
                }
                if let Some(p) = d.get("path").and_then(|v| v.as_str()) {
                    path_deps.push(PathBuf::from(p));
                }
            }
        }
        let targets = pkg
            .get("targets")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let target_kind = |t: &serde_json::Value, want: &str| {
            t.get("kind")
                .and_then(|k| k.as_array())
                .map(|arr| arr.iter().any(|x| x.as_str() == Some(want)))
                .unwrap_or(false)
        };
        let has_bin_entry = targets.iter().any(|t| target_kind(t, "bin"));
        let has_main_rs = crate_dir.join("src").join("main.rs").exists();
        let has_bin_dir = crate_dir.join("src").join("bin").is_dir();
        let has_bin = has_bin_entry || has_main_rs || has_bin_dir;
        let lib_target = targets
            .iter()
            .find(|t| target_kind(t, "lib") || target_kind(t, "proc-macro"));
        let has_lib_rs = crate_dir.join("src").join("lib.rs").exists();
        let has_lib = lib_target.is_some() || has_lib_rs;
        // Names are bindings: only a TRUE rename (beyond hyphen
        // normalization) rides the lib_name field.
        let lib_name = lib_target
            .and_then(|t| t.get("name").and_then(|v| v.as_str()))
            .filter(|ln| ln.replace('-', "_") != name.replace('-', "_"))
            .map(String::from);
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
        if crates.contains_key(&name) {
            eprintln!(
                "[characterize] package name collision across units: `{}` (unit {}) keeps the first ingest",
                name, unit_key
            );
            continue;
        }
        members.push(name.clone());
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
                version,
                lib_name,
                renames,
                unit: unit_key.to_string(),
            },
        );
    }
    (members, path_deps)
}

/// What: the .gitmodules entry (path, url) whose path equals or
/// prefixes `rel`, when one exists.
fn submodule_for<'a>(
    rel: &str,
    gitmodules: &'a [(String, String)],
) -> Option<&'a (String, String)> {
    gitmodules
        .iter()
        .find(|(sub_path, _)| rel == sub_path || rel.starts_with(&format!("{}/", sub_path)))
}

/// What: provenance for a unit root: the matching .gitmodules entry
/// (URL + gitlink rev from `git ls-tree`) when the unit sits at or
/// under a submodule path, else plain in-repo vendored source.
fn provenance_for(
    root: &Path,
    unit_rel: &str,
    gitmodules: &[(String, String)],
) -> UnitProvenance {
    match submodule_for(unit_rel, gitmodules) {
        Some((sub_path, url)) => UnitProvenance::Submodule {
            url: url.clone(),
            rev: gitlink_rev(root, sub_path),
        },
        None => UnitProvenance::InRepo,
    }
}

/// What: parse `.gitmodules` into (path, url) pairs. Line-level scan
/// (section headers + `path =` / `url =` assignments); absent or
/// unreadable file yields an empty list.
fn parse_gitmodules(root: &Path) -> Vec<(String, String)> {
    let text = match fs::read_to_string(root.join(".gitmodules")) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur_path: Option<String> = None;
    let mut cur_url: Option<String> = None;
    let flush = |p: &mut Option<String>, u: &mut Option<String>, out: &mut Vec<(String, String)>| {
        if let (Some(path), Some(url)) = (p.take(), u.take()) {
            out.push((path, url));
        }
        *p = None;
        *u = None;
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            flush(&mut cur_path, &mut cur_url, &mut out);
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            match k.trim() {
                "path" => cur_path = Some(v.trim().to_string()),
                "url" => cur_url = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    flush(&mut cur_path, &mut cur_url, &mut out);
    out
}

/// What: the gitlink commit SHA recorded for a submodule path in the
/// host repo's HEAD tree (`git ls-tree HEAD -- <path>` mode 160000),
/// readable whether or not the submodule is populated. None when the
/// path is not a gitlink or git is unavailable.
fn gitlink_rev(root: &Path, sub_path: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["ls-tree", "HEAD", "--", sub_path])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next()?;
    let mut parts = first.split_whitespace();
    let mode = parts.next()?;
    let kind = parts.next()?;
    let sha = parts.next()?;
    if mode == "160000" && kind == "commit" {
        Some(sha.to_string())
    } else {
        None
    }
}
