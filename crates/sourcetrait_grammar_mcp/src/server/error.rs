use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// What: a diagnostic's location. `path` is the file RELATIVE TO THE LIBRARIES
/// DIR (`<library>/<file>`, e.g. `geo/shape/area.nu`, `geo/mod.nu`) for a
/// library-validation diagnostic, and `null` for a run/interact body
/// diagnostic (located in the body, no file). `position` is `[line, col]`,
/// 1-based; `[0, 0]` is file-level (no specific line).
///
/// Why: replaces the prior `Where` + `WhereSource` carrier with the flat wire
/// shape the agent consumes. The whole `Source` is `null` (on the
/// `Diagnostic`) for a non-located diagnostic (an eval timeout, an
/// unregistered-library error).
///
/// Where: built by the body lint (`server::lint`, `path: None`) and the
/// library validator (`server::library`, `path: Some("<library>/<rel>")`);
/// serialized as part of a `Diagnostic`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct Source {
    pub path: Option<String>,
    pub position: [usize; 2],
}

/// What: one agent-facing diagnostic row. `kind` is the namespaced taxonomy
/// string (`library::*`, `lint::*`, `thread::*`, ...); `source` is the
/// location (`null` when non-located); `message` carries the human detail.
/// `severity` selects the envelope bucket and is NOT serialized.
///
/// Why: the single unified diagnostic type collapses the former
/// `library::Violation` (structural validator) and `lint::LintViolation`
/// (body lint) into one shape. The former typed per-variant `data`
/// (timeout_ms, passed/registered, reason, ...) folds into `message` -- no
/// loss, it is text either way -- so there is no separate `data` field, and
/// the bucket (`errors` vs `warnings`) carries severity instead of a row
/// field.
///
/// Where: produced by `server::lint` (body lint, severity Error) and
/// `server::library::validate_library_source` (structural Error +
/// `lint::summary_length` Warning); carried by `Error::LibraryViolations` /
/// `Error::LintViolations` and `tool::library::CheckSummary`; rendered to the
/// wire by `error_to_call_result`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct Diagnostic {
    pub kind: String,
    #[serde(skip)]
    pub severity: Severity,
    pub source: Option<Source>,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn error(
        kind: impl Into<String>,
        source: Option<Source>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            severity: Severity::Error,
            source,
            message: message.into(),
        }
    }

    pub(crate) fn warning(
        kind: impl Into<String>,
        source: Option<Source>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            severity: Severity::Warning,
            source,
            message: message.into(),
        }
    }

    pub(crate) fn bucket(diagnostics: Vec<Diagnostic>) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        for d in diagnostics {
            match d.severity {
                Severity::Error => errors.push(d),
                Severity::Warning => warnings.push(d),
            }
        }
        (errors, warnings)
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    LibraryNotRegistered {
        library: String,
    },
    LibraryAlreadyRegistered {
        library: String,
    },
    LibraryInvalidName {
        library: String,
        reason: String,
    },
    LibraryNameDenied {
        library: String,
    },
    LibraryInvalidModulePath {
        module_path: String,
        reason: String,
    },
    LibrarySourceMissing {
        path: String,
    },
    LibrarySourcePathMismatch {
        library: String,
        passed: String,
        registered: String,
    },
    LibraryViolations {
        diagnostics: Vec<Diagnostic>,
    },
    LibraryInvalidAction {
        action: String,
    },
    FunctionNotDefined {
        library: String,
        module_path: String,
        name: String,
    },
    LintViolations {
        diagnostics: Vec<Diagnostic>,
    },
    SchemaInvalid {
        reason: String,
    },
    NamepathInvalid {
        namepath: String,
        reason: String,
    },
    RerunInvalidNonce {
        nonce: String,
        reason: String,
    },
    RerunBodyMissing {
        nonce: String,
    },
    RerunBodyDecode {
        nonce: String,
        reason: String,
    },
    ChannelStart {
        reason: String,
    },
    ChannelNotOpen,
    ThreadDispatch {
        reason: String,
    },
    ThreadTimeout {
        timeout_ms: u64,
    },
    ThreadReturnedError {
        reason: String,
    },
    Internal {
        phase: String,
        reason: String,
    },
}

impl Error {
    fn kind_str(&self) -> &'static str {
        match self {
            Self::LibraryNotRegistered { .. } => "library::not_registered",
            Self::LibraryAlreadyRegistered { .. } => "library::already_registered",
            Self::LibraryInvalidName { .. } => "library::invalid_name",
            Self::LibraryNameDenied { .. } => "library::name_denied",
            Self::LibraryInvalidModulePath { .. } => "library::invalid_module_path",
            Self::LibrarySourceMissing { .. } => "library::source_missing",
            Self::LibrarySourcePathMismatch { .. } => "library::source_path_mismatch",
            Self::LibraryInvalidAction { .. } => "library::invalid_action",
            Self::FunctionNotDefined { .. } => "function::not_defined",
            Self::SchemaInvalid { .. } => "schema::invalid",
            Self::NamepathInvalid { .. } => "namepath::invalid",
            Self::RerunInvalidNonce { .. } => "rerun::invalid_nonce",
            Self::RerunBodyMissing { .. } => "rerun::body_missing",
            Self::RerunBodyDecode { .. } => "rerun::body_decode",
            Self::ChannelStart { .. } => "channel::start",
            Self::ChannelNotOpen => "channel::not_open",
            Self::ThreadDispatch { .. } => "thread::dispatch",
            Self::ThreadTimeout { .. } => "thread::timeout",
            Self::ThreadReturnedError { .. } => "thread::returned_error",
            Self::Internal { .. } => "internal",
            Self::LibraryViolations { .. } | Self::LintViolations { .. } => {
                unreachable!("violation variants render via bucket, not kind_str")
            }
        }
    }

    fn message(&self) -> String {
        match self {
            Self::LibraryNotRegistered { library } => {
                format!("library `{library}` is not registered")
            }
            Self::LibraryAlreadyRegistered { library } => {
                format!("library `{library}` is already registered")
            }
            Self::LibraryInvalidName { library, reason } => {
                format!("invalid library name `{library}`: {reason}")
            }
            Self::LibraryNameDenied { library } => {
                format!("library name `{library}` is reserved and cannot be used")
            }
            Self::LibraryInvalidModulePath {
                module_path,
                reason,
            } => format!("invalid module path `{module_path}`: {reason}"),
            Self::LibrarySourceMissing { path } => {
                format!("library source path is missing or not a directory: {path}")
            }
            Self::LibrarySourcePathMismatch {
                library,
                passed,
                registered,
            } => format!(
                "source_dir mismatch for `{library}`: passed `{passed}`, registered `{registered}`"
            ),
            Self::LibraryInvalidAction { action } => format!("unknown library action `{action}`"),
            Self::FunctionNotDefined {
                library,
                module_path,
                name,
            } => format!("function `{name}` is not defined in `{library}:{module_path}`"),
            Self::SchemaInvalid { reason } => format!("invalid schema: {reason}"),
            Self::NamepathInvalid { namepath, reason } => {
                format!("invalid namepath `{namepath}`: {reason}")
            }
            Self::RerunInvalidNonce { nonce, reason } => {
                format!("invalid nonce `{nonce}`: {reason}")
            }
            Self::RerunBodyMissing { nonce } => {
                format!("no cached run body for nonce `{nonce}`")
            }
            Self::RerunBodyDecode { nonce, reason } => {
                format!("failed to decode cached run body for nonce `{nonce}`: {reason}")
            }
            Self::ChannelStart { reason } => format!("cannot start the channel hub: {reason}"),
            Self::ChannelNotOpen => {
                "the channel is not open; call channel_open() first".to_string()
            }
            Self::ThreadDispatch { reason } => format!("eval dispatch failed: {reason}"),
            Self::ThreadTimeout { timeout_ms } => format!("eval timed out after {timeout_ms} ms"),
            Self::ThreadReturnedError { reason } => reason.clone(),
            Self::Internal { phase, reason } => format!("internal error [{phase}]: {reason}"),
            Self::LibraryViolations { .. } | Self::LintViolations { .. } => {
                unreachable!("violation variants render via bucket, not message")
            }
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Internal {
            phase: "io".to_string(),
            reason: e.to_string(),
        }
    }
}

/// What: top-of-wire envelope -- the structured error body keyed under the
/// `error` field of `structured_content`.
///
/// Why: the single typed shape that lands in `CallToolResult::structured_content`
/// for every tool-execution error; the `error` wrapper is the
/// success-vs-error discriminator the agent branches on.
///
/// Where: produced by `error_to_call_result`; consumed by integration tests
/// asserting `result.structuredContent.error`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// What: the unified error body -- `errors` + `warnings` diagnostic lists plus
/// an optional `nonce`. Both lists are always present (possibly empty);
/// `nonce` is omitted when absent.
///
/// Why: every failure response now carries the same `{ errors, warnings,
/// nonce? }` shape regardless of the originating condition; the bucket split
/// conveys severity without a per-row `severity` field.
///
/// Where: built by `error_to_call_result`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct ErrorBody {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

pub(crate) fn error_to_call_result(
    error: Error,
    nonce: Option<Nonce>,
) -> mcp::CallToolResult {
    let (errors, warnings) = match error {
        Error::LibraryViolations { diagnostics } | Error::LintViolations { diagnostics } => {
            Diagnostic::bucket(diagnostics)
        }
        single => {
            let diagnostic = Diagnostic::error(single.kind_str(), None, single.message());
            (vec![diagnostic], Vec::new())
        }
    };
    let envelope = ErrorEnvelope {
        error: ErrorBody {
            errors,
            warnings,
            nonce: nonce.map(|n| n.to_string()),
        },
    };
    let mut r = mcp::CallToolResult::default();
    r.structured_content = json::to_value(&envelope).ok();
    r
}
