use crate::*;

/// What: the binary's entry function. Parses argv via clap derive,
/// dispatches to the per-category scan run modules.
///
/// Why: main.rs is 2-3 lines per `mem:developer` rust_main_thin, so
/// the real orchestration belongs here. clap handles --help /
/// --version / arg-parse errors itself (exit-on-error path), so
/// run() only surfaces domain errors via `Error`.
///
/// Where: called from main.rs; surfaces Error to the binary entry
/// point.
pub fn run() -> std::result::Result<(), Error> {
    let cli = Cli::parse();
    let calibration = load_calibration(cli.calibration_path.as_deref())?;
    let _templates = Templates::new(cli.templates_path.clone());
    match cli.command {
        Command::Scan { scan } => dispatch_scan(scan),
        Command::Characterize {
            workspace_root,
            out_dir,
        } => characterize(&workspace_root, &out_dir, &calibration),
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
        } => scan_items(&workspace_root, &out_dir),
    }
}
