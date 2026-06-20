use crate::*;

/// What: the crate's error type for the fallible parts of writing the
/// session payload artifact - serializing the YAML and the filesystem
/// writes / symlink relinks.
///
/// Why: the side-write is best-effort (rendering the statusline is the
/// primary, must-not-fail concern), so these errors are produced by the
/// store helpers and swallowed by run(); centralizing them in one enum
/// keeps the helper signatures uniform (ClaudelineResult) per the crate
/// convention.
///
/// Where: produced in store.rs (persist / clear_latest), propagated with
/// `?`; consumed (and ignored, fail-soft) in run.rs.
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum ClaudelineError {
    #[snafu(display("serialize payload to YAML: {source}"))]
    SerializeYaml { source: serde_norway::Error },

    #[snafu(display("filesystem op failed at {path:?}: {source}"))]
    Fs { path: PathBuf, source: io::Error },

    #[snafu(display("no XDG cache directory available"))]
    NoCacheDir,
}

pub(crate) type ClaudelineResult<T> = Result<T, ClaudelineError>;
