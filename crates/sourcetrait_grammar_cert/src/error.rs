pub type Result<T> = std::result::Result<T, CertError>;

/// Failures generating, installing or verifying the local CA and its leaf.
///
/// Every variant that wraps an underlying failure KEEPS its source rather than
/// flattening it to a message. Certificate work is exactly where the cause matters:
/// "generation failed" is useless next to the rcgen error that says which parameter
/// was rejected, and an install that cannot read a key needs to distinguish a missing
/// file from a permission denial.
#[derive(Debug, snafu::Snafu)]
pub enum CertError {
    #[snafu(display("{context}: {source}"))]
    Io {
        context: String,
        source: std::io::Error,
    },

    #[snafu(display("{context}: {source}"))]
    Rcgen {
        context: String,
        source: rcgen::Error,
    },

    #[snafu(display("reading config {path}: {source}"))]
    ConfigRead {
        path: String,
        source: std::io::Error,
    },

    #[snafu(display("parsing config {path}: {source}"))]
    ConfigParse {
        path: String,
        source: toml::de::Error,
    },

    #[snafu(display("invalid subject_alt_names entry {value:?}: {reason}"))]
    InvalidSan { value: String, reason: String },

    #[snafu(display("{path} already exists"))]
    CertsDirExists { path: String },

    #[snafu(display("{path} is not a directory produced by `generate`: {reason}"))]
    NotACertDir { path: String, reason: String },

    #[snafu(display("`{command}` failed ({status}): {stderr}"))]
    Command {
        command: String,
        status: String,
        stderr: String,
    },

    #[snafu(display("cannot resolve user {user}: {reason}"))]
    UnknownUser { user: String, reason: String },

    #[snafu(display("changing ownership of {path} to uid {uid}: {source}"))]
    Chown {
        path: String,
        uid: u32,
        source: nix::errno::Errno,
    },

    #[snafu(display("{message}"))]
    Msg { message: String },
}

impl CertError {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn rcgen(context: impl Into<String>, source: rcgen::Error) -> Self {
        Self::Rcgen {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn msg(message: impl Into<String>) -> Self {
        Self::Msg {
            message: message.into(),
        }
    }
}
