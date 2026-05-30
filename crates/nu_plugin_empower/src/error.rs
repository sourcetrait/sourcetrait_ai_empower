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

impl From<NuPluginEmpowerError> for nu_protocol::LabeledError {
    fn from(err: NuPluginEmpowerError) -> Self {
        nu_protocol::LabeledError::new(err.to_string())
    }
}
