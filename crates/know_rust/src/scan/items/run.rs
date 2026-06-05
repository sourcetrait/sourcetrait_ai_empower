use crate::*;

/// What: parse a Rust source string under a relative file path and run
/// the items walker over it, returning the per-file facts. Returns
/// None if parse fails (mirrors `syn::parse_str` semantics).
///
/// Why: integration tests need to assert against the walker's output
/// on hand-crafted snippets without writing the workspace harness
/// each time. This wraps the internal FileWalker so the public API
/// stays narrow.
///
/// Where: called by tests/scan_items_integration.rs; not used in the
/// production CLI path (which goes through scan_workspace).
pub fn scan_source(rel: &str, src: &str) -> Option<FileLevelFacts> {
    let parsed: syn::File = syn::parse_str(src).ok()?;
    let walker = FileWalker::new(rel.to_string());
    Some(walker.walk_file(&parsed))
}

/// What: orchestrate the `know_rust scan items` invocation. Walk the
/// workspace via `scan_workspace`, take the returned `ItemFacts`,
/// serialize to `know_rust_items.json` in the output directory, and
/// log the file count to stderr.
///
/// Why: characterize.py consumes the produced JSON as its per-file
/// lex+structure facts source (impls / traits / types / fns / etc.).
/// Separating orchestration from `scan_workspace` lets the walker
/// stay focused on per-file emission + aggregation and lets the
/// entry point be invoked directly from integration tests via the
/// crate's pub re-export.
///
/// Where: dispatched by `crate::run::run` via the `ScanCommand::Items`
/// match arm; called from `tests/scan_items_integration.rs`.
pub fn scan_items(
    workspace_root: &Path,
    out_dir: &Path,
) -> std::result::Result<(), Error> {
    let facts = scan_workspace(workspace_root);
    let out_path = out_dir.join("know_rust_items.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[know_rust scan items] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}
