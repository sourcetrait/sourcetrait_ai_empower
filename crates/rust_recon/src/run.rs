use crate::*;

/// What: the binary's entry function. Parses argv, walks the
/// workspace, serializes Facts as scan.json into the output dir.
///
/// Why: the binary's job is intentionally narrow - run main.rs is
/// 2-3 lines per `mem:developer` rust_main_thin, so the real
/// orchestration belongs here.
///
/// Where: called from main.rs; surfaces Error to the binary entry
/// point.
pub fn run() -> std::result::Result<(), Error> {
    let cli = Cli::from_args().map_err(|source| Error::CliParse { source })?;
    let facts = walk_workspace(&cli.workspace_root)?;
    let out_path = cli.out_dir.join("scan.json");
    let json = serde_json::to_string_pretty(&facts)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[rust_recon] {} files scanned, {} parse failed, wrote {}",
        facts.files_scanned,
        facts.files_parse_failed,
        out_path.display()
    );
    Ok(())
}
