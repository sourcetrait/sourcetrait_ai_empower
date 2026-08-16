use crate::*;

/// Configuration for a `Bed`; the engine is fully configured through it.
///
/// `env` is the ONLY environment a script sees - an explicit merge-in; the
/// host process environment is never gathered, and nothing a script sets ever
/// merges back out. `timeout` bounds each run via cooperative interrupt.
/// Defaults: empty env, no timeout.
#[derive(Clone, Debug, Default)]
pub struct BedConfig {
    pub(crate) env: Vec<(String, Value)>,
    pub(crate) timeout: Option<Duration>,
}

impl BedConfig {
    /// Start a builder over the default (empty-env, no-timeout) configuration.
    pub fn builder() -> BedConfigBuilder {
        BedConfigBuilder::default()
    }
}

/// Builder facade for `BedConfig`.
#[derive(Clone, Debug, Default)]
pub struct BedConfigBuilder {
    env: Vec<(String, Value)>,
    timeout: Option<Duration>,
}

impl BedConfigBuilder {
    /// Add one environment variable scripts will see (repeat per variable).
    /// The value is a nushell `Value` (e.g. a list for a PATH-like variable);
    /// a later duplicate key shadows an earlier one.
    pub fn env(
        mut self,
        key: impl Into<String>,
        value: Value,
    ) -> Self {
        self.env.push((key.into(), value));
        self
    }

    /// Bound each run; on expiry the run is cooperatively interrupted and
    /// surfaces `BedError::Timeout`.
    pub fn timeout(
        mut self,
        timeout: Duration,
    ) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn build(self) -> BedConfig {
        BedConfig {
            env: self.env,
            timeout: self.timeout,
        }
    }
}
