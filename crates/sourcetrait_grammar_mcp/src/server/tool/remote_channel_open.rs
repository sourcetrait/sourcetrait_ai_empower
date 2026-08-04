use crate::*;

/// Parameters for `remote_channel_open()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RemoteChannelOpenParams {
    /// The `[[remote]]` alias to open, from `.grammar/mcp/remotes.toml`.
    pub alias: String,
}

#[mcp::tool_router(router = remote_channel_open_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Open a configured remote link by alias; returns nothing while it opens. mcp/remote/Connected (or Disconnected {error}) lands on the Channel."
    )]
    pub(crate) async fn remote_channel_open(
        &self,
        mcp::Parameters(p): mcp::Parameters<RemoteChannelOpenParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let Some(entry) = config().remote.get(&p.alias).cloned() else {
            return Ok(error_to_call_result(
                Error::RemoteInvalidParams {
                    reason: format!(
                        "no remote `{}` in .grammar/mcp/remotes.toml",
                        p.alias,
                    ),
                },
                None,
            ));
        };
        if self
            .remote_links
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&p.alias)
        {
            return Ok(error_to_call_result(
                Error::RemoteAlreadyOpen {
                    alias: p.alias.clone(),
                },
                None,
            ));
        }
        open_remote(self.mcp_nom.to_string(), entry);
        Ok(mcp::CallToolResult::default())
    }
}
