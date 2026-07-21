use crate::*;

/// Parameters for `channel_close()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ChannelCloseParams {}

/// RFC 6455 normal closure. Sending a real close frame is what lets the agent tell a
/// planned teardown from a host that simply died, which arrives as a bare 1006.
const CLOSE_PLANNED: u16 = 1000;
const PLANNED_REASON: &str = "channel closed";

#[mcp::tool_router(router = channel_close_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(description = "Close the host's packet channel.")]
    pub(crate) async fn channel_close(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ChannelCloseParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Idempotent, like library(uninstall): closing what is already closed is the
        // requested state, not a failure.
        self.channel.close(CLOSE_PLANNED, PLANNED_REASON);
        Ok(mcp::CallToolResult::default())
    }
}
