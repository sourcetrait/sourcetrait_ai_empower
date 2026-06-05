use crate::*;

/// What: walk every non-test, non-bench `.rs` file under one crate's
/// directory, compute SLOC across the cfg(test)-stripped source, and
/// drain the per-file items index (pre-built from the workspace items
/// scan) into a per-crate `CrateAggregate`.
///
/// Why: characterize.py's `scan_crate` aggregates per-crate facts +
/// SLOC; the python implementation walks via `rglob("*.rs")` and
/// drains the global `_ITEMS_BY_FILE` index. The Rust port walks via
/// `walkdir` + explicit path sort so the within-crate iteration order
/// is deterministic (matching the items walker's path-sorted emission
/// order).
///
/// Where: called from `crate::characterize::run::characterize` once
/// per crate in find_crates iteration order.
pub fn scan_crate(
    root: &Path,
    crate_name: &str,
    crate_dir: &str,
    crate_dirs: &indexmap::IndexMap<String, String>,
    items_by_file: &std::collections::HashMap<String, ItemFile>,
) -> CrateAggregate {
    let mut agg = CrateAggregate::default();
    let base = if crate_dir == "." {
        root.to_path_buf()
    } else {
        root.join(crate_dir)
    };

    let mut rs_files: Vec<PathBuf> = walkdir::WalkDir::new(&base)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
        .filter(|e| !e.path().components().any(|c| c.as_os_str() == "target"))
        .map(|e| e.path().to_path_buf())
        .collect();
    rs_files.sort();

    for path in rs_files {
        let rel_to_base = match path.strip_prefix(&base) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel_to_base.components().any(|c| {
            let s = c.as_os_str().to_str().unwrap_or("");
            s == "tests" || s == "benches"
        }) {
            continue;
        }

        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        let owning = resolve_crate_for_file(&rel, crate_dirs);
        if owning != crate_name {
            continue;
        }

        let src = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let src = strip_cfg_test(&src);
        agg.sloc += compute_sloc(&src);

        if let Some(f) = items_by_file.get(&rel) {
            for r in &f.impls {
                agg.impls.push(serde_json::to_value(r).expect("ImplEntry serializes"));
            }
            for r in &f.traits {
                agg.traits.push(serde_json::to_value(r).expect("TraitEntry serializes"));
            }
            for r in &f.types {
                agg.types.push(serde_json::to_value(r).expect("TypeEntry serializes"));
            }
            for r in &f.fns {
                agg.fns.push(serde_json::to_value(r).expect("FnEntry serializes"));
            }
            for r in &f.uses {
                agg.uses.push(serde_json::to_value(r).expect("UseEntry serializes"));
            }
            for r in &f.macros {
                agg.macros.push(serde_json::to_value(r).expect("MacroEntry serializes"));
            }
            for r in &f.derives {
                agg.derives.push(serde_json::to_value(r).expect("DeriveEntry serializes"));
            }
            for r in &f.macro_defs {
                agg.macro_defs.push(serde_json::to_value(r).expect("MacroDefEntry serializes"));
            }
            for r in &f.mods {
                agg.mods.push(serde_json::to_value(r).expect("ModEntry serializes"));
            }
            for r in &f.type_usages {
                agg.type_usages.push(serde_json::to_value(r).expect("TypeUsageEntry serializes"));
            }
            for r in &f.example_type_usages {
                agg.example_type_usages.push(serde_json::to_value(r).expect("TypeUsageEntry serializes"));
            }
        }
    }
    agg
}
