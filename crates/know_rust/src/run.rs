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
    let templates = Templates::new(cli.templates_path.clone());
    let weights = match cli.weights_path.as_deref() {
        Some(p) => Some(load_weights(p)?),
        None => None,
    };
    match cli.command {
        Command::Scan { scan } => dispatch_scan(scan),
        Command::Characterize {
            workspace_root,
            out_dir,
        } => characterize(&workspace_root, &out_dir, &calibration),
        Command::Emit {
            workspace_root,
            out_dir,
        } => emit(
            &workspace_root,
            &out_dir,
            &calibration,
            &templates,
            weights.as_ref(),
        ),
        Command::Measure { measure } => dispatch_measure(measure),
        Command::RustdocOverlay {
            workspace_root,
            orientation_dir,
            package,
        } => rustdoc_overlay(&workspace_root, &orientation_dir, package.as_deref()),
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

fn dispatch_measure(measure: MeasureCommand) -> std::result::Result<(), Error> {
    match measure {
        MeasureCommand::Demand {
            consumer_root,
            target_out_dir,
            out,
        } => measure_demand(&consumer_root, &target_out_dir, out.as_deref()),
        MeasureCommand::Consumers {
            consumer_repos,
            pairs_root,
            outputs_root,
            weights_out,
        } => measure_consumers(
            &consumer_repos,
            &pairs_root,
            &outputs_root,
            weights_out.as_deref(),
        ),
        MeasureCommand::Overlap {
            samples_dir,
            ground_truth,
        } => measure_overlap(&samples_dir, &ground_truth),
    }
}
