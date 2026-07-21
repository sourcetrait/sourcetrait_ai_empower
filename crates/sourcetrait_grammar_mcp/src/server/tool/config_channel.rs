use crate::*;

/// Parameters for `config_channel()` - a PARTIAL update: supply only what should move.
///
/// Windows are INTEGER SECONDS because MCP arguments cross as JSON, which cannot carry a
/// nu `duration`; the `10s` form lives in the model and the docs.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ConfigChannelParams {
    /// Window for the SOFT threshold, in whole seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spam_warn_window_secs: Option<u64>,
    /// Sends within that window before one warning is issued for an origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spam_warn_rate: Option<u32>,
    /// Window for the HARD threshold, in whole seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spam_error_window_secs: Option<u64>,
    /// Sends within that window before the origin is refused and stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spam_error_rate: Option<u32>,
}

/// Success result of `config_channel()` - the policy now IN FORCE, whether or not this
/// call changed it, so a caller never has to assume its own update took.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ConfigChannelEnvelope {
    pub spam_warn_window_secs: u64,
    pub spam_warn_rate: u32,
    pub spam_error_window_secs: u64,
    pub spam_error_rate: u32,
}

impl From<SpamThresholds> for ConfigChannelEnvelope {
    fn from(spam: SpamThresholds) -> Self {
        Self {
            spam_warn_window_secs: spam.warn_window.as_secs(),
            spam_warn_rate: spam.warn_rate,
            spam_error_window_secs: spam.error_window.as_secs(),
            spam_error_rate: spam.error_rate,
        }
    }
}

#[mcp::tool_router(router = config_channel_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Read or adjust the channel's send-rate thresholds at runtime.",
        output_schema = mcp::schema_for_type::<ConfigChannelEnvelope>()
    )]
    pub(crate) async fn config_channel(
        &self,
        mcp::Parameters(p): mcp::Parameters<ConfigChannelParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Only the thresholds are settable here. The port and the cert dir cannot change
        // under a live hub, so they stay where they are set once, at startup.
        match self.channel.set_thresholds(
            p.spam_warn_window_secs,
            p.spam_warn_rate,
            p.spam_error_window_secs,
            p.spam_error_rate,
        ) {
            Ok(spam) => envelope_to_structured(&ConfigChannelEnvelope::from(spam)),
            Err(reason) => Ok(error_to_call_result(Error::SchemaInvalid { reason }, None)),
        }
    }
}
