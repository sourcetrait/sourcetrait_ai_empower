use crate::*;

/// What: the binary's entry function. Parses argv via clap derive,
/// dispatches to subcommand implementations.
///
/// Why: the binary's job is intentionally narrow - main.rs is 2-3
/// lines per `mem:developer` rust_main_thin, so the real
/// orchestration belongs here. clap handles --help / --version /
/// arg-parse errors itself (exit-on-error path), so run() only
/// surfaces domain errors.
///
/// Where: called from main.rs; surfaces Error to the binary entry
/// point.
pub fn run() -> std::result::Result<(), Error> {
    let cli = <Cli as clap::Parser>::parse();
    match cli.command {
        Command::Scan { scan } => dispatch_scan(scan),
    }
}

fn dispatch_scan(scan: ScanCommand) -> std::result::Result<(), Error> {
    match scan {
        ScanCommand::Usages {
            workspace_root,
            out_dir,
        } => scan_usages(&workspace_root, &out_dir),
        ScanCommand::Items {
            workspace_root,
            out_dir,
        } => scan_workspace(&workspace_root, &out_dir),
    }
}

fn scan_usages(
    workspace_root: &std::path::Path,
    out_dir: &std::path::Path,
) -> std::result::Result<(), Error> {
    let facts = walk_workspace(workspace_root)?;
    let out_path = out_dir.join("recon_usages.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    std::fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[rust_recon scan usages] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}
