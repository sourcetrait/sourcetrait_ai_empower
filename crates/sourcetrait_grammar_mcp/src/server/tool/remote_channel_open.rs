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
        description = "Open a configured remote link by alias; blocks on bind (listener) or connect (connector, 20s) and returns synchronously - void on success, an error envelope on failure. A listener then emits mcp/remote/Connected when a peer pairs; Disconnected fires on any established-link teardown."
    )]
    pub(crate) async fn remote_channel_open(
        &self,
        mcp::Parameters(p): mcp::Parameters<RemoteChannelOpenParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let Some(entry) = config().remotes.by_alias.get(&p.alias).cloned() else {
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
        match open_remote_blocking(self.mcp_nom.to_string(), entry).await {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(reason) => Ok(error_to_call_result(
                Error::RemoteOpenFailed {
                    alias: p.alias,
                    reason,
                },
                None,
            )),
        }
    }
}
