use crate::*;

/// Parameters for `remote_channel_close()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RemoteChannelCloseParams {
    /// The alias of the open link to close.
    pub alias: String,
}

#[mcp::tool_router(router = remote_channel_close_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(description = "Close an open mTLS link to a remote grammar host.")]
    pub(crate) async fn remote_channel_close(
        &self,
        mcp::Parameters(p): mcp::Parameters<RemoteChannelCloseParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let entry = self
            .remote_links
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&p.alias);
        match entry {
            Some(entry) => {
                // Cancel the drivers; the link's open task observes the end,
                // deregisters, and emits mcp/remote/Disconnected on the Channel.
                entry.handle.cancel();
                Ok(mcp::CallToolResult::default())
            }
            None => Ok(error_to_call_result(
                GrammarMcpError::RemoteNotOpen { alias: p.alias },
                None,
            )),
        }
    }
}
