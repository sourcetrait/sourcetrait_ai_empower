use crate::*;

/// What: parsed CLI arguments via clap derive. Parent `scan`
/// subcommand with noun children. `usages` captures AST-derived
/// cross-item type-reference signals; `items` captures per-file
/// lex+structure facts (ported from rustscan.py).
///
/// Why: clap derive gives subcommand structure + automatic
/// help/version + future extensibility. Surface mirrors the python
/// CLI structure; future iterations port more subcommands
/// (characterize / emit / measure-overlap / etc.) as top-level
/// peers of `scan`.
///
/// Where: parsed in run.rs entrypoint via `Cli::parse()`; dispatched
/// by subcommand match.
#[derive(clap::Parser, Debug)]
#[command(
    name = "know_rust",
    version,
    about = "Rust source scanner for the know_rust orientation pipeline",
)]
pub(crate) struct Cli {
    /// Path to a custom calibration.toml; defaults to the embedded
    /// calibration shipped with the binary.
    #[arg(short = 'c', long = "calibration", global = true)]
    pub(crate) calibration_path: Option<PathBuf>,

    /// Path to a custom templates root containing `prompts/` and
    /// `templates/` subdirectories; defaults to the embedded
    /// templates shipped with the binary.
    #[arg(short = 't', long = "templates", global = true)]
    pub(crate) templates_path: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum Command {
    /// Source scans (AST cross-item usages + lex/structure facts).
    Scan {
        #[command(subcommand)]
        scan: ScanCommand,
    },
    /// Characterize a workspace: build the dependency graph, run the
    /// item + usage scans in-process, aggregate per-crate, compute
    /// pattern_metrics + workspace_shape + use-classification, and
    /// write facts.json + fingerprint.json (plus the intermediate
    /// know_rust_items.json + know_rust_usages.json) to the output
    /// directory.
    Characterize {
        /// Workspace root to characterize.
        workspace_root: PathBuf,
        /// Output directory; facts.json + fingerprint.json (plus
        /// know_rust_items.json + know_rust_usages.json) are written
        /// here.
        out_dir: PathBuf,
    },
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum ScanCommand {
    /// AST-derived cross-item usage signals (fn-sig + field +
    /// type-alias + method-ref usages). Writes know_rust_usages.json
    /// in the output directory.
    Usages {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; know_rust_usages.json is written here.
        out_dir: PathBuf,
    },
    /// Per-file lex+structure facts (impls, derives, types,
    /// traits, fns, macros, uses, mods, seams, type_usages).
    /// Writes know_rust_items.json in the output directory.
    Items {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; know_rust_items.json is written here.
        out_dir: PathBuf,
    },
}
