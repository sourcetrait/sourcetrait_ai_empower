use crate::*;

/// What: discriminant-only Copy enum for the error kind taxonomy.
/// One variant per `Error` variant; serialized as the colon-separated
/// snake_case kind string (`"library::not_registered"`, etc.).
///
/// Why: lets callers branch on / log the error kind without holding
/// the typed data. `Error` carries the data; `ErrorKind` is the
/// peel-it-off discriminant.
///
/// Where: returned by `Error::kind()`; useful in match arms that
/// don't bind the data fields, in logging, and in tests.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, ser::Serialize, ser::Deserialize, schema::JsonSchema)]
pub enum ErrorKind {
    #[serde(rename = "library::not_registered")]       LibraryNotRegistered,
    #[serde(rename = "library::already_registered")]   LibraryAlreadyRegistered,
    #[serde(rename = "library::invalid_name")]         LibraryInvalidName,
    #[serde(rename = "library::invalid_module_path")]  LibraryInvalidModulePath,
    #[serde(rename = "library::test_suffix_required")] LibraryTestSuffixRequired,
    #[serde(rename = "library::source_missing")]       LibrarySourceMissing,
    #[serde(rename = "library::wrong_kind")]           LibraryWrongKind,
    #[serde(rename = "library::violations")]           LibraryViolations,
    #[serde(rename = "function::not_defined")]         FunctionNotDefined,
    #[serde(rename = "function::invalid_name")]        FunctionInvalidName,
    #[serde(rename = "lint::violations")]              LintViolations,
    #[serde(rename = "schema::invalid")]               SchemaInvalid,
    #[serde(rename = "closure::invalid_rerun_id")]     ClosureInvalidRerunId,
    #[serde(rename = "closure::cache_missing")]        ClosureCacheMissing,
    #[serde(rename = "closure::cache_decode")]         ClosureCacheDecode,
    #[serde(rename = "worker::dispatch")]              WorkerDispatch,
    #[serde(rename = "worker::timeout")]               WorkerTimeout,
    #[serde(rename = "worker::returned_error")]        WorkerReturnedError,
    #[serde(rename = "internal")]                      Internal,
}

/// What: fieldful error enum carrying the typed per-variant data
/// shape. Variants are 1:1 with `ErrorKind`. Serde tag+content
/// produces the wire shape `{"kind": "namespace::reason", "data":
/// {...}}` on the wire, wrapped by `ErrorBody`/`ErrorEnvelope`.
///
/// Why: every error site builds a typed `Error` value; the wire
/// shape, the JsonSchema, and the Rust call site stay in lockstep.
/// No stringly-keyed disambiguation, no rendered-text middle layer.
///
/// Where: every tool handler in `server::tool.rs` builds an `Error`
/// when it would have built an `Err(ErrorData)` before, and routes
/// through `error_to_call_result`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
#[serde(tag = "kind", content = "data")]
pub enum Error {
    #[serde(rename = "library::not_registered")]
    LibraryNotRegistered { library: String },

    #[serde(rename = "library::already_registered")]
    LibraryAlreadyRegistered { library: String },

    #[serde(rename = "library::invalid_name")]
    LibraryInvalidName { library: String, reason: String },

    #[serde(rename = "library::invalid_module_path")]
    LibraryInvalidModulePath { module_path: String, reason: String },

    #[serde(rename = "library::test_suffix_required")]
    LibraryTestSuffixRequired { library: String },

    #[serde(rename = "library::source_missing")]
    LibrarySourceMissing { path: String },

    #[serde(rename = "library::wrong_kind")]
    LibraryWrongKind { library: String },

    #[serde(rename = "library::violations")]
    LibraryViolations {
        structural: Vec<Violation>,
        lint: Vec<LintViolation>,
    },

    #[serde(rename = "function::not_defined")]
    FunctionNotDefined { library: String, module_path: String, name: String },

    #[serde(rename = "function::invalid_name")]
    FunctionInvalidName { name: String, reason: String },

    #[serde(rename = "lint::violations")]
    LintViolations { violations: Vec<LintViolation> },

    #[serde(rename = "schema::invalid")]
    SchemaInvalid { reason: String },

    #[serde(rename = "closure::invalid_rerun_id")]
    ClosureInvalidRerunId { rerun_id: String, reason: String },

    #[serde(rename = "closure::cache_missing")]
    ClosureCacheMissing { rerun_id: String },

    #[serde(rename = "closure::cache_decode")]
    ClosureCacheDecode { rerun_id: String, reason: String },

    #[serde(rename = "worker::dispatch")]
    WorkerDispatch { reason: String },

    #[serde(rename = "worker::timeout")]
    WorkerTimeout { timeout_ms: u64 },

    #[serde(rename = "worker::returned_error")]
    WorkerReturnedError { reason: String },

    #[serde(rename = "internal")]
    Internal { phase: String, reason: String },
}

impl Error {
    /// What: returns the `ErrorKind` discriminant for this `Error`
    /// value. 1:1 mapping; no data peeled off.
    ///
    /// Why: callers that want to log the kind, branch on the kind,
    /// or compare two errors for kind-equality without binding the
    /// per-variant data fields.
    ///
    /// Where: not on the hot path; used by error logging seams +
    /// integration tests that compare against `ErrorKind`.
    #[allow(dead_code)]
    pub fn kind(&self) -> ErrorKind {
        use ErrorKind as K;
        match self {
            Self::LibraryNotRegistered { .. } => K::LibraryNotRegistered,
            Self::LibraryAlreadyRegistered { .. } => K::LibraryAlreadyRegistered,
            Self::LibraryInvalidName { .. } => K::LibraryInvalidName,
            Self::LibraryInvalidModulePath { .. } => K::LibraryInvalidModulePath,
            Self::LibraryTestSuffixRequired { .. } => K::LibraryTestSuffixRequired,
            Self::LibrarySourceMissing { .. } => K::LibrarySourceMissing,
            Self::LibraryWrongKind { .. } => K::LibraryWrongKind,
            Self::LibraryViolations { .. } => K::LibraryViolations,
            Self::FunctionNotDefined { .. } => K::FunctionNotDefined,
            Self::FunctionInvalidName { .. } => K::FunctionInvalidName,
            Self::LintViolations { .. } => K::LintViolations,
            Self::SchemaInvalid { .. } => K::SchemaInvalid,
            Self::ClosureInvalidRerunId { .. } => K::ClosureInvalidRerunId,
            Self::ClosureCacheMissing { .. } => K::ClosureCacheMissing,
            Self::ClosureCacheDecode { .. } => K::ClosureCacheDecode,
            Self::WorkerDispatch { .. } => K::WorkerDispatch,
            Self::WorkerTimeout { .. } => K::WorkerTimeout,
            Self::WorkerReturnedError { .. } => K::WorkerReturnedError,
            Self::Internal { .. } => K::Internal,
        }
    }
}

/// What: blanket `From<io::Error>` so library impls can use `?` on
/// `fs::*` and `process::Command::*` operations to bubble io
/// failures up as `Error::Internal { phase: "io", reason: ... }`
/// without per-site `.map_err(...)`.
///
/// Why: substrate operations (mkdir, fs::write, git commit
/// subprocess, etc.) produce io::Error values whose message strings
/// already carry enough context for the agent; collapsing them all
/// to `Error::Internal` keeps the wire taxonomy tight. Callers that
/// want a more specific `phase` field do an explicit `.map_err(|e|
/// Error::Internal { phase: "<better>", reason: e.to_string() })`.
///
/// Where: every `?` on an io-fallible call inside the `*_impl`
/// functions in `server::library.rs`.
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Internal {
            phase: "io".to_string(),
            reason: e.to_string(),
        }
    }
}

/// What: top-of-wire envelope -- carries the structured error body
/// keyed under the `error` field of `structured_content`.
///
/// Why: this is the single typed shape that lands in
/// `CallToolResult::structured_content` for every tool-execution
/// error. The seam (`error_to_call_result`) keeps the wire format
/// uniform across handlers.
///
/// Where: produced by `error_to_call_result`; consumed by
/// integration tests asserting
/// `resp["result"]["structuredContent"]["error"]`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct ErrorBody {
    #[serde(flatten)]
    pub error: Error,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// What: where an in-source violation lives. `position` is
/// `[line, col]` (1-based). `source` is an optional typed tag for
/// disambiguating across multiple violation contexts; absence means
/// "body" (the default lint context for `run` / `interact`).
///
/// Why: typed source replaces the prior `Option<String>` so the
/// agent can branch on the source kind without parsing rendered
/// text. Walker code passes `Where` values; LintViolation variants
/// inline its fields on the wire (via the variant's own `position`
/// + `source` fields rather than nesting under a `location` key).
///
/// Where: built by the lint walker in `server::lint.rs`; consumed
/// when constructing `LintViolation::HardcodedVariable` /
/// `DeniedCommand` variants.
#[derive(Debug, Clone)]
pub struct Where {
    pub position: [usize; 2],
    pub source: Option<WhereSource>,
}

/// What: typed lint-violation source tag. `Mod(rel_path)` for the
/// library validator's per-file source; `Def(fn_name)` reserved for
/// future per-function-body lints; `Other(free_text)` catch-all.
///
/// Why: distinct typed variants let the agent branch on the
/// violation's source kind without parsing the rendered string.
/// The wire shape is ALWAYS a plain string (via hand-rolled
/// `Serialize` -> `Display`); the enum is internal Rust typing.
///
/// Where: stored in `Where::source`; rendered to wire by the lint
/// validator emit sites.
#[derive(Debug, Clone)]
pub enum WhereSource {
    Mod(String),
    #[allow(dead_code)]
    Def(String),
    #[allow(dead_code)]
    Other(String),
}

impl std::fmt::Display for WhereSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mod(s) => write!(f, "mod {s}"),
            Self::Def(s) => write!(f, "def {s}"),
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl ser::Serialize for WhereSource {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl schema::JsonSchema for WhereSource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("WhereSource")
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <String as schema::JsonSchema>::json_schema(g)
    }
}

/// What: builds a SUCCESS-shape `CallToolResult` carrying the typed
/// error envelope in `structured_content`. No `is_error` set; no
/// `Err(ErrorData)` returned. The agent receives a successful tool
/// response with `structuredContent.error.kind == "<X>::<Y>"` and an
/// empty `content` array. Domain error path; deviates from MCP
/// 2025-11-25 `server/tools.md` SHOULD identically to
/// `envelope_to_structured` -- see the "Content::text omission
/// deviation note" block in `server/tool/common.rs` for the policy.
///
/// Why: pairs with `envelope_to_structured` to give one uniform
/// `structured_content`-only wire shape across success and error
/// semantics, so agents read fields from one place regardless of
/// outcome. `Err(mcp::ErrorData)` JSON-RPC errors are reserved for
/// genuine MCP-layer protocol failures (the current handler set
/// never hits that path).
///
/// Where: called by every tool handler in `server::tool.rs` on any
/// tool-execution error path.
pub(crate) fn error_to_call_result(
    error: Error,
    nonce: Option<lib_empower::Nonce>,
) -> mcp::CallToolResult {
    let envelope = ErrorEnvelope {
        error: ErrorBody {
            error,
            nonce: nonce.map(|n| n.to_string()),
        },
    };
    let mut r = mcp::CallToolResult::default();
    r.structured_content = json::to_value(&envelope).ok();
    r
}
