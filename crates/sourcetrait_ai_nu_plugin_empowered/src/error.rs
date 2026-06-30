#[allow(unused)]
pub type NuPluginEmpowerResult<T> = Result<T, NuPluginEmpowerError>;

#[derive(Debug, snafu::Snafu)]
pub enum NuPluginEmpowerError {
    #[snafu(whatever, display("{message}"))]
    Whatever {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error + Send + Sync>, Some)))]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

/// Build a message-only plugin error (no source) for the liquid + schema seams.
pub(crate) fn nu_plugin_error(message: impl Into<String>) -> NuPluginEmpowerError {
    NuPluginEmpowerError::Whatever {
        message: message.into(),
        source: None,
    }
}

/// Map any displayable error to a `LabeledError` anchored at the call head.
pub(crate) fn labeled_error(
    message: impl std::fmt::Display,
    head: nu_protocol::Span,
) -> nu_protocol::LabeledError {
    let message = message.to_string();
    nu_protocol::LabeledError::new(message.clone()).with_label(message, head)
}

impl From<NuPluginEmpowerError> for nu_protocol::LabeledError {
    fn from(err: NuPluginEmpowerError) -> Self {
        nu_protocol::LabeledError::new(err.to_string())
    }
}

/// What: result alias for the markdown structural-query surface, with
/// `MarkdownError` baked in as the error variant.
///
/// Why: `md::find` writes `MarkdownResult<Vec<...>>` without re-spelling the
/// error type at every call site; the same `std::io::Result<T>` convention.
///
/// Where: returned by `md::find`; the `empowered eye md find` command maps it
/// to a `LabeledError` at the call head.
pub(crate) type MarkdownResult<T> = Result<T, MarkdownError>;

/// What: error variants for the markdown structural-query surface -
/// file-read failures and invalid regex patterns.
///
/// Why: errors live in `error.rs` regardless of the module they represent (the
/// workspace lib convention); snafu derives Display + the `.context()`
/// selectors (`ReadFileSnafu` / `InvalidPatternSnafu`) consumed at the
/// `md::find` call sites. Moved here from `sourcetrait_ai_lib_empower` when
/// lib_empower dissolved to its base62 + consts remainder.
///
/// Where: the error half of `MarkdownResult<T>`; matched by the `md` unit
/// tests; mapped to a nushell `LabeledError` by the eye md find command.
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum MarkdownError {
    #[snafu(display("could not read markdown file: {}", path.display()))]
    ReadFile {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("invalid regex pattern: {pattern}"))]
    InvalidPattern {
        pattern: String,
        source: regex::Error,
    },
}
