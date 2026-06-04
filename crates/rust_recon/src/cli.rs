use crate::*;
use ext_clap::*;

/// What: parsed CLI arguments via clap derive. Parent `scan`
/// subcommand with noun children. `usages` captures AST-derived
/// cross-item type-reference signals; `items` follows at phase 3+
/// (per-file lex+structure facts ported from rustscan.py).
///
/// Why: clap derive gives subcommand structure + automatic
/// help/version + future extensibility. Surface mirrors the python
/// CLI structure; future iterations port more subcommands
/// (characterize / emit / measure-overlap / etc.) as top-level
/// peers of `scan`.
///
/// Where: parsed in run.rs entrypoint via Cli::parse(); dispatched
/// by subcommand match.
#[derive(Parser, Debug)]
#[command(
    name = "rust_recon",
    version,
    about = "Rust source scanner for the rust_recon orientation pipeline",
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Source scans (AST cross-item usages + lex/structure facts).
    Scan {
        #[command(subcommand)]
        scan: ScanCommand,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum ScanCommand {
    /// AST-derived cross-item usage signals (fn-sig + field +
    /// type-alias + method-ref usages). Writes recon_usages.json
    /// in the output directory.
    Usages {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; recon_usages.json is written here.
        out_dir: PathBuf,
    },
    /// Per-file lex+structure facts (impls, derives, types,
    /// traits, fns, macros, uses, mods, seams, type_usages).
    /// Writes recon_items.json in the output directory. Phase 3
    /// stub; phase 4 ports rustscan.py.
    Items {
        /// Workspace root to scan.
        workspace_root: PathBuf,
        /// Output directory; recon_items.json is written here.
        out_dir: PathBuf,
    },
}
