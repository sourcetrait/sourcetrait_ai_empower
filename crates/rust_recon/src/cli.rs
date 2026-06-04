use crate::*;
use snafu::Snafu;

/// What: parsed command-line arguments. Two positionals: the workspace
/// root to scan + the output directory where scan.json is written.
///
/// Why: the scanner is invoked from characterize.py with the same
/// workspace-root + output-directory contract. Keeping the surface
/// minimal eases the Python wrapper. Phase 1 of the 0.0.34 sweep
/// retains the bare-positional shape; Phase 2 wraps this under a
/// clap-derived `scan usages` subcommand.
///
/// Where: built by `Cli::from_args()` in run.rs; consumed by walk()
/// and the JSON write at the end.
#[derive(Debug)]
pub(crate) struct Cli {
    pub(crate) workspace_root: PathBuf,
    pub(crate) out_dir: PathBuf,
}

#[derive(Debug, Snafu)]
pub enum CliError {
    #[snafu(display(
        "usage: rust_recon <workspace_root> <out_dir>"
    ))]
    BadArgs,
}

impl Cli {
    pub(crate) fn from_args() -> std::result::Result<Self, CliError> {
        let mut args = std::env::args().skip(1);
        let root = args.next().ok_or(CliError::BadArgs)?;
        let out = args.next().ok_or(CliError::BadArgs)?;
        Ok(Cli {
            workspace_root: PathBuf::from(root),
            out_dir: PathBuf::from(out),
        })
    }
}
