use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A diagnostic's location; `path` is rigs-dir-relative, or null in a body.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct Source {
    pub path: Option<String>,
    pub position: [usize; 2],
}

/// One agent-facing diagnostic row.
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
    RigNotRegistered {
        rig: String,
    },
    RigAlreadyRegistered {
        rig: String,
    },
    RigInvalidName {
        rig: String,
        reason: String,
    },
    RigNameDenied {
        rig: String,
    },
    RigInvalidModulePath {
        module_path: String,
        reason: String,
    },
    RigSourceMissing {
        path: String,
    },
    RigSourcePathMismatch {
        rig: String,
        passed: String,
        registered: String,
    },
    RigViolations {
        diagnostics: Vec<Diagnostic>,
    },
    RigInvalidAction {
        action: String,
    },
    FunctionNotDefined {
        rig: String,
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
    PurviewInvalidId {
        id: String,
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
    ChannelNotClaimed,
    ChannelPeerGone,
    RemoteInvalidParams {
        reason: String,
    },
    RemoteAlreadyOpen {
        alias: String,
    },
    RemoteNotOpen {
        alias: String,
    },
    ThreadDispatch {
        reason: String,
    },
    ThreadTimeout {
        timeout_ms: u64,
    },
    ThreadReturnedError {
        reason: String,
    },
    ModuleCircularImport {
        files: String,
    },
    Internal {
        phase: String,
        reason: String,
    },
}

impl Error {
    fn kind_str(&self) -> &'static str {
        match self {
            Self::RigNotRegistered { .. } => "rig::not_registered",
            Self::RigAlreadyRegistered { .. } => "rig::already_registered",
            Self::RigInvalidName { .. } => "rig::invalid_name",
            Self::RigNameDenied { .. } => "rig::name_denied",
            Self::RigInvalidModulePath { .. } => "rig::invalid_module_path",
            Self::RigSourceMissing { .. } => "rig::source_missing",
            Self::RigSourcePathMismatch { .. } => "rig::source_path_mismatch",
            Self::RigInvalidAction { .. } => "rig::invalid_action",
            Self::FunctionNotDefined { .. } => "function::not_defined",
            Self::SchemaInvalid { .. } => "schema::invalid",
            Self::NamepathInvalid { .. } => "namepath::invalid",
            Self::PurviewInvalidId { .. } => "purview::invalid_id",
            Self::RerunInvalidNonce { .. } => "rerun::invalid_nonce",
            Self::RerunBodyMissing { .. } => "rerun::body_missing",
            Self::RerunBodyDecode { .. } => "rerun::body_decode",
            Self::ChannelStart { .. } => "channel::start",
            Self::ChannelNotOpen => "channel::not_open",
            Self::ChannelNotClaimed => "channel::not_claimed",
            Self::ChannelPeerGone => "channel::peer_gone",
            Self::RemoteInvalidParams { .. } => "remote::invalid_params",
            Self::RemoteAlreadyOpen { .. } => "remote::already_open",
            Self::RemoteNotOpen { .. } => "remote::not_open",
            Self::ThreadDispatch { .. } => "thread::dispatch",
            Self::ThreadTimeout { .. } => "thread::timeout",
            Self::ThreadReturnedError { .. } => "thread::returned_error",
            Self::ModuleCircularImport { .. } => "module::circular_import",
            Self::Internal { .. } => "internal",
            Self::RigViolations { .. } | Self::LintViolations { .. } => {
                unreachable!("violation variants render via bucket, not kind_str")
            }
        }
    }

    fn message(&self) -> String {
        match self {
            Self::RigNotRegistered { rig } => {
                format!("rig `{rig}` is not registered")
            }
            Self::RigAlreadyRegistered { rig } => {
                format!("rig `{rig}` is already registered")
            }
            Self::RigInvalidName { rig, reason } => {
                format!("invalid rig name `{rig}`: {reason}")
            }
            Self::RigNameDenied { rig } => {
                format!("rig name `{rig}` is reserved and cannot be used")
            }
            Self::RigInvalidModulePath {
                module_path,
                reason,
            } => format!("invalid module path `{module_path}`: {reason}"),
            Self::RigSourceMissing { path } => {
                format!("rig source path is missing or not a directory: {path}")
            }
            Self::RigSourcePathMismatch {
                rig,
                passed,
                registered,
            } => format!(
                "source_dir mismatch for `{rig}`: passed `{passed}`, registered `{registered}`"
            ),
            Self::RigInvalidAction { action } => format!("unknown rig action `{action}`"),
            Self::FunctionNotDefined {
                rig,
                module_path,
                name,
            } => format!("function `{name}` is not defined in `{rig}:{module_path}`"),
            Self::SchemaInvalid { reason } => format!("invalid schema: {reason}"),
            Self::NamepathInvalid { namepath, reason } => {
                format!("invalid namepath `{namepath}`: {reason}")
            }
            Self::PurviewInvalidId { id, reason } => {
                format!("invalid purview id `{id}`: {reason}")
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
            Self::ChannelNotClaimed => "nothing has connected to the channel yet; start \
                 a Monitor at the wss endpoint channel_open() returned, see the \
                 channel/Open packet, then verify"
                .to_string(),
            Self::ChannelPeerGone => "the channel's peer connection is gone; call \
                 channel_close() then channel_open() for a fresh channel"
                .to_string(),
            Self::RemoteInvalidParams { reason } => {
                format!("invalid remote link parameters: {reason}")
            }
            Self::RemoteAlreadyOpen { alias } => format!("remote link `{alias}` is already open"),
            Self::RemoteNotOpen { alias } => format!("no open remote link `{alias}`"),
            Self::ThreadDispatch { reason } => format!("eval dispatch failed: {reason}"),
            Self::ThreadTimeout { timeout_ms } => format!("eval timed out after {timeout_ms} ms"),
            Self::ThreadReturnedError { reason } => reason.clone(),
            Self::ModuleCircularImport { files } => files.clone(),
            Self::Internal { phase, reason } => format!("internal error [{phase}]: {reason}"),
            Self::RigViolations { .. } | Self::LintViolations { .. } => {
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

/// Top-of-wire envelope: the structured error body keyed under `error`.
#[derive(Debug, Clone, ser::Serialize, schema::JsonSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// The unified error body: `errors`, `warnings`, and an optional nonce.
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
        Error::RigViolations { diagnostics } | Error::LintViolations { diagnostics } => {
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
