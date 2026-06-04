/// What: error type unified across the scanner pipeline.
///
/// Why: a single Snafu enum lets the binary entry point format
/// errors uniformly without per-call-site bespoke handling.
///
/// Where: returned by run() + threaded through walk + scan; the
/// main.rs converts to a process exit + stderr message.
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("read failed for {path:?}: {source}"))]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("parse failed for {path:?}: {source}"))]
    Parse {
        path: std::path::PathBuf,
        source: syn::Error,
    },

    #[snafu(display("write failed for {path:?}: {source}"))]
    Write {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("serialize failed: {source}"))]
    Serialize { source: serde_json::Error },
}

pub type Result<T> = std::result::Result<T, Error>;
