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
        // A DELIBERATE close prunes what the channel wrote. The caller is declaring it
        // is done, so dropping the attachments is intentional rather than inferred, and
        // it leaves less for the system pruner (the_user). Scoped to THIS path on
        // purpose - the shutdown close, the verify-timer expiry and the
        // unverified-emit teardown are host-initiated, and none of them is the caller
        // saying it is finished.
        //
        // Best-effort: the close already happened and this tool cannot fail. The path
        // is host-derived (set by `channel_open` under $XDGX_SHM_DIR), never
        // caller-supplied, which is what makes a recursive remove bounded here.
        if let Some(inbox) = self.channel.take_inbox()
            && let Err(e) = fs::remove_dir_all(&inbox)
        {
            eprintln!("grammar: inbox prune failed at {}: {e}", inbox.display());
        }
        Ok(mcp::CallToolResult::default())
    }
}
