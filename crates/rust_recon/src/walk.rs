use crate::*;

/// What: walk the workspace root recursively, parse every `.rs` file
/// via syn, accumulate FnSigUsage + FieldUsage + TypeAliasUsage
/// entries into Facts.
///
/// Why: characterize.py drives the workspace boundary (Cargo.toml
/// detection, per-crate aggregation); this binary's job is just to
/// produce the AST-derived supplemental signal across all source.
/// Aggregation by crate happens Python-side.
///
/// Where: called by run() with the cli-supplied workspace_root.
pub(crate) fn walk_workspace(root: &Path) -> Result<Facts> {
    let mut facts = Facts {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_target_dir(e.path()))
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let rel = match p.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        match scan_file(&src, &rel) {
            Ok(ff) => {
                facts.ast_fn_sig_usages.extend(ff.fn_sig_usages);
                facts.ast_field_usages.extend(ff.field_usages);
                facts.ast_type_alias_usages.extend(ff.type_alias_usages);
                facts.ast_method_ref_usages.extend(ff.method_ref_usages);
                facts.files_scanned += 1;
            }
            Err(_) => {
                facts.files_parse_failed += 1;
            }
        }
    }
    Ok(facts)
}

fn is_target_dir(p: &Path) -> bool {
    p.components().any(|c| {
        c.as_os_str().to_str() == Some("target")
    })
}
