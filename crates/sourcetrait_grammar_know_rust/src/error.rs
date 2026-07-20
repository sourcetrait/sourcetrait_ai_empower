use crate::*;

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
        path: PathBuf,
        source: io::Error,
    },

    #[snafu(display("parse failed for {path:?}: {source}"))]
    Parse {
        path: PathBuf,
        source: syn::Error,
    },

    #[snafu(display("write failed for {path:?}: {source}"))]
    Write {
        path: PathBuf,
        source: io::Error,
    },

    #[snafu(display("serialize failed: {source}"))]
    Serialize { source: serde_json::Error },

    #[snafu(display("toml parse failed for {path:?}: {source}"))]
    TomlParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[snafu(display("liquid template {name}: {source}"))]
    Liquid {
        name: String,
        source: liquid::Error,
    },

    #[snafu(display("template not found: {name}"))]
    TemplateNotFound { name: String },

    #[snafu(display("rustdoc overlay requires a nightly cargo toolchain: {reason}"))]
    ToolchainMissing { reason: String },

    #[snafu(display("cargo +nightly rustdoc failed: {reason}"))]
    RustdocFailed { reason: String },

    #[snafu(display("unknown rustdoc format_version={version}; aborting per the hard-nightly requirement"))]
    UnknownRustdocFormatVersion { version: String },

    #[snafu(display("demand misses: {names} name(s), {pairs} pair(s) uncovered"))]
    DemandMisses { names: usize, pairs: usize },

    #[snafu(display(
        "unknown documentation profile `{name}`; declare [profile.{name}] in calibration.toml (the built-in default is `author`)"
    ))]
    UnknownProfile { name: String },
}

pub type Result<T> = std::result::Result<T, Error>;
