use crate::*;

/// Parameters for `channel_close()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ChannelCloseParams {}

/// RFC 6455 normal closure.
const CLOSE_PLANNED: u16 = 1000;
const PLANNED_REASON: &str = "channel closed";

#[mcp::tool_router(router = channel_close_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(description = "Close the host's packet channel.")]
    pub(crate) async fn channel_close(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ChannelCloseParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        self.channel.close(CLOSE_PLANNED, PLANNED_REASON);
        if let Some(inbox) = self.channel.take_inbox()
            && let Err(e) = fs::remove_dir_all(&inbox)
        {
            eprintln!("grammar: inbox prune failed at {}: {e}", inbox.display());
        }
        Ok(mcp::CallToolResult::default())
    }
}
