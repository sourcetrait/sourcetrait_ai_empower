use crate::*;

/// What: orchestrate the `know_rust scan usages` invocation. Walk the
/// workspace once, collect cross-item AST usage signals into
/// `UsageFacts`, serialize to `know_rust_usages.json` in the output
/// directory, and log the file count to stderr.
///
/// Why: characterize.py consumes the produced JSON as its AST usage
/// signal source (fn signatures + struct fields + type aliases +
/// method references). Separating orchestration from the workspace
/// walker keeps the walker focused on per-file aggregation and lets
/// the entry point be invoked directly from integration tests via
/// the crate's pub re-export.
///
/// Where: dispatched by `crate::run::run` via the `ScanCommand::Usages`
/// match arm; called from `tests/scan_usages_integration.rs`.
pub fn scan_usages(
    workspace_root: &Path,
    out_dir: &Path,
) -> std::result::Result<(), Error> {
    let facts = walk_workspace(workspace_root)?;
    let out_path = out_dir.join("know_rust_usages.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[know_rust scan usages] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}
