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
