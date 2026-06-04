use crate::*;

/// What: orchestrate the `rust_recon scan items` invocation. Walk every
/// non-test, non-bench `.rs` file under `workspace_root`, parse it via
/// `syn::parse_file`, drive a `FileWalker` over the AST, and merge each
/// file's facts into the aggregate `ItemsFacts` written to
/// `recon_items.json`.
///
/// Why: characterize.py consumes the produced JSON file as its source
/// of per-file lex+structure facts (impls / traits / types / fns / etc.
/// keyed by file). Centralizing the workspace walk + file iteration
/// here keeps the visitor focused on per-file emission.
///
/// Where: called from `crate::run::run` via the `Scan::Items` clap
/// subcommand match.
pub(crate) fn scan_workspace(
    workspace_root: &std::path::Path,
    out_dir: &std::path::Path,
) -> std::result::Result<(), Error> {
    let mut facts = ItemsFacts {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };
    let mut aggregate_seams: std::collections::HashMap<SeamKind, usize> = std::collections::HashMap::new();
    let rs_files = collect_rs_files(workspace_root);
    for (path, rel) in &rs_files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: syn::File = match syn::parse_str(&src) {
            Ok(f) => f,
            Err(_) => {
                facts.files_parse_failed += 1;
                continue;
            }
        };
        facts.files_scanned += 1;
        let walker = FileWalker::new(rel.clone());
        let file_facts = walker.walk_file(&parsed);
        merge_file_facts(&mut facts, &mut aggregate_seams, file_facts);
    }
    let mut seams_wire: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (k, n) in aggregate_seams {
        if n > 0 {
            seams_wire.insert(k.wire_key().to_string(), n);
        }
    }
    facts.seams = seams_wire;
    let out_path = out_dir.join("recon_items.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    std::fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[rust_recon scan items] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}

/// Collect every `.rs` file under `workspace_root`, sorted by relative
/// path for determinism. Skips `target/`, `tests/`, and `benches/`
/// segments anywhere in the path.
fn collect_rs_files(workspace_root: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
    let mut rs_files = Vec::new();
    for entry in walkdir::WalkDir::new(workspace_root)
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
        let rel = match p.strip_prefix(workspace_root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.iter().any(|s| *s == "tests" || *s == "benches") {
            continue;
        }
        rs_files.push((p.to_path_buf(), rel));
    }
    rs_files.sort_by(|a, b| a.1.cmp(&b.1));
    rs_files
}

fn is_target_dir(p: &std::path::Path) -> bool {
    p.components()
        .any(|c| c.as_os_str().to_str() == Some("target"))
}

/// Drain a per-file accumulator into the workspace-level `ItemsFacts`
/// and merge its seam counters into the workspace `SeamKind` map.
fn merge_file_facts(
    facts: &mut ItemsFacts,
    aggregate_seams: &mut std::collections::HashMap<SeamKind, usize>,
    file_facts: FileLevelFacts,
) {
    facts.impls.extend(file_facts.impls);
    facts.traits.extend(file_facts.traits);
    facts.types.extend(file_facts.types);
    facts.fns.extend(file_facts.fns);
    facts.mods.extend(file_facts.mods);
    facts.uses.extend(file_facts.uses);
    facts.macros.extend(file_facts.macros);
    facts.macro_defs.extend(file_facts.macro_defs);
    facts.attrs.extend(file_facts.attrs);
    facts.derives.extend(file_facts.derives);
    facts.type_usages.extend(file_facts.type_usages);
    facts.example_type_usages.extend(file_facts.example_type_usages);
    facts.doc_count += file_facts.doc_count;
    for (kind, n) in file_facts.seams {
        *aggregate_seams.entry(kind).or_default() += n;
    }
}
