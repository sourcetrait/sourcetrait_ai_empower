use crate::*;

/// What: severity bucket for a `Diagnostic`. `Error` rows block (a commit, a
/// run) and land in the envelope's `errors`; `Warning` rows advise and land in
/// `warnings`.
///
/// Why: the wire conveys severity through the `errors[]` vs `warnings[]` split,
/// so this field is `#[serde(skip)]` on `Diagnostic` (absent from the wire AND
/// the emitted JSON schema). It exists only for the Rust-side `bucket`
/// partition + `ValidationResult::is_empty` (errors block, warnings don't).
///
/// Where: set by `Diagnostic::error` / `Diagnostic::warning`; read by
/// `Diagnostic::bucket` and `library::ValidationResult::is_empty`.
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
/// `Diagnostic`) for a non-located diagnostic (a worker timeout, an
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
/// string (`library::*`, `lint::*`, `worker::*`, ...); `source` is the
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
    /// Build an Error-severity diagnostic (blocks; lands in `errors`).
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

    /// Build a Warning-severity diagnostic (advisory; lands in `warnings`).
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

    /// What: partition diagnostics into `(errors, warnings)` by severity,
    /// preserving order within each bucket.
    ///
    /// Why: the wire envelope (`error_to_call_result`) and the `library(check)`
    /// success summary both present diagnostics split by severity; this is the
    /// single partition point.
    ///
    /// Where: called by `error_to_call_result` (the violation-bearing Error
    /// variants) and `tool::library::check_summary_from`.
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

/// What: typed error for ergonomic single-condition construction at the call
/// sites. The two violation-bearing variants carry `Vec<Diagnostic>` (already
/// the unified rows); every other variant carries its typed data and is
/// rendered to ONE error-bucket `Diagnostic` at the wire seam.
///
/// Why: keeping a typed `Error` lets the impls build + bubble conditions with
/// `?` and matchable values, while `error_to_call_result` is the single place
/// that flattens any `Error` to the uniform `{ errors, warnings, nonce? }`
/// wire shape. There is no longer a serde-tagged wire form on `Error` itself
/// -- the kind strings live in `kind_str`, the human text in `message`.
///
/// Where: built by every tool handler + `server::library` impl on a failure
/// path; consumed by `error_to_call_result`.
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
    LibraryTestSuffixRequired {
        library: String,
    },
    LibrarySourceMissing {
        path: String,
    },
    LibrarySourcePathMismatch {
        library: String,
        passed: String,
        registered: String,
    },
    /// commit() / library(check) validation: the unified validator diagnostics
    /// (structural Error rows + `lint::summary_length` Warning rows). Buckets
    /// at the wire seam.
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
    /// run() / interact() body lint: the body-lint diagnostics (all severity
    /// Error). Buckets at the wire seam (warnings empty).
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
    ClosureInvalidRerunId {
        rerun_id: String,
        reason: String,
    },
    ClosureCacheMissing {
        rerun_id: String,
    },
    ClosureCacheDecode {
        rerun_id: String,
        reason: String,
    },
    WorkerDispatch {
        reason: String,
    },
    WorkerTimeout {
        timeout_ms: u64,
    },
    WorkerReturnedError {
        reason: String,
    },
    Internal {
        phase: String,
        reason: String,
    },
}

impl Error {
    /// What: the namespaced `kind` string for the single-condition variants
    /// (`library::not_registered`, `worker::timeout`, ...).
    ///
    /// Why: with no serde tag on `Error`, the kind taxonomy lives here. The two
    /// violation variants carry their own per-`Diagnostic` kinds and are
    /// bucketed directly, so they never reach this method.
    ///
    /// Where: called by `error_to_call_result` for every non-violation variant.
    fn kind_str(&self) -> &'static str {
        match self {
            Self::LibraryNotRegistered { .. } => "library::not_registered",
            Self::LibraryAlreadyRegistered { .. } => "library::already_registered",
            Self::LibraryInvalidName { .. } => "library::invalid_name",
            Self::LibraryNameDenied { .. } => "library::name_denied",
            Self::LibraryInvalidModulePath { .. } => "library::invalid_module_path",
            Self::LibraryTestSuffixRequired { .. } => "library::test_suffix_required",
            Self::LibrarySourceMissing { .. } => "library::source_missing",
            Self::LibrarySourcePathMismatch { .. } => "library::source_path_mismatch",
            Self::LibraryInvalidAction { .. } => "library::invalid_action",
            Self::FunctionNotDefined { .. } => "function::not_defined",
            Self::SchemaInvalid { .. } => "schema::invalid",
            Self::NamepathInvalid { .. } => "namepath::invalid",
            Self::ClosureInvalidRerunId { .. } => "closure::invalid_rerun_id",
            Self::ClosureCacheMissing { .. } => "closure::cache_missing",
            Self::ClosureCacheDecode { .. } => "closure::cache_decode",
            Self::WorkerDispatch { .. } => "worker::dispatch",
            Self::WorkerTimeout { .. } => "worker::timeout",
            Self::WorkerReturnedError { .. } => "worker::returned_error",
            Self::Internal { .. } => "internal",
            Self::LibraryViolations { .. } | Self::LintViolations { .. } => {
                unreachable!("violation variants render via bucket, not kind_str")
            }
        }
    }

    /// What: render the single-condition variant's typed data into the human
    /// `message` string (the former per-variant `data` folds in here -- no
    /// loss, it is text either way).
    ///
    /// Why: the unified `Diagnostic` carries one `message` instead of a typed
    /// `data` object; the detail (timeout_ms, passed/registered, reason, ...)
    /// goes into prose here.
    ///
    /// Where: called by `error_to_call_result` for every non-violation variant.
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
            Self::LibraryTestSuffixRequired { library } => {
                format!("library `{library}` must end with `_test` on the test variant")
            }
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
            Self::ClosureInvalidRerunId { rerun_id, reason } => {
                format!("invalid rerun_id `{rerun_id}`: {reason}")
            }
            Self::ClosureCacheMissing { rerun_id } => {
                format!("no cached closure for rerun_id `{rerun_id}`")
            }
            Self::ClosureCacheDecode { rerun_id, reason } => {
                format!("failed to decode cached closure `{rerun_id}`: {reason}")
            }
            Self::WorkerDispatch { reason } => format!("worker dispatch failed: {reason}"),
            Self::WorkerTimeout { timeout_ms } => format!("worker timed out after {timeout_ms} ms"),
            Self::WorkerReturnedError { reason } => reason.clone(),
            Self::Internal { phase, reason } => format!("internal error [{phase}]: {reason}"),
            Self::LibraryViolations { .. } | Self::LintViolations { .. } => {
                unreachable!("violation variants render via bucket, not message")
            }
        }
    }
}

/// What: blanket `From<io::Error>` so library impls can use `?` on `fs::*` and
/// `process::Command::*` operations to bubble io failures up as
/// `Error::Internal { phase: "io", reason: ... }` without per-site `.map_err`.
///
/// Why: substrate operations (mkdir, fs::write, git commit subprocess) produce
/// io::Error values whose message strings already carry enough context;
/// collapsing them to `Error::Internal` keeps the taxonomy tight. Callers
/// wanting a more specific `phase` do an explicit `.map_err`.
///
/// Where: every `?` on an io-fallible call inside the `*_impl` functions in
/// `server::library`.
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

/// What: builds a SUCCESS-shape `CallToolResult` carrying the unified error
/// envelope (`{ error: { errors, warnings, nonce? } }`) in
/// `structured_content`. The violation-bearing `Error` variants bucket their
/// carried diagnostics; every other variant renders to a single error-bucket
/// `Diagnostic` (`kind` from `kind_str`, `source: None`, `message` from
/// `message`). No `is_error`; no `Err(ErrorData)`.
///
/// Why: one uniform `structured_content`-only wire shape across success and
/// error semantics, so the agent reads `error.errors[]` / `error.warnings[]`
/// from one place. The nonce is attached when the dispatch had already
/// allocated a per-call log dir -- the agent can fetch
/// `<cache>/<kind>/<nonce>/{stdout,stderr}`.
///
/// Where: called by every tool handler on a tool-execution error path.
pub(crate) fn error_to_call_result(
    error: Error,
    nonce: Option<lib_empower::Nonce>,
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
