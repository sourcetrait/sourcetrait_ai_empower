use crate::*;

/// Parameters for `remote_channel_close()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RemoteChannelCloseParams {
    /// The alias of the open link to close.
    pub alias: String,
}

/// How long a link's close handshake may take before its drivers are aborted.
const REMOTE_CLOSE_TIMEOUT: tk::TkDuration = tk::TkDuration::from_secs(5);

#[mcp::tool_router(router = remote_channel_close_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(description = "Close an open mTLS link to a remote grammar host.")]
    pub(crate) async fn remote_channel_close(
        &self,
        mcp::Parameters(p): mcp::Parameters<RemoteChannelCloseParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let entry = self.remote_links.lock().await.remove(&p.alias);
        match entry {
            Some(mut entry) => {
                entry.handle.close(REMOTE_CLOSE_TIMEOUT).await;
                Ok(mcp::CallToolResult::default())
            }
            None => Ok(error_to_call_result(
                Error::RemoteNotOpen { alias: p.alias },
                None,
            )),
        }
    }
}
