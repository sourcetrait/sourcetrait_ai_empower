use crate::*;

/// Parameters for `remote_channels()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RemoteChannelsParams {}

/// One open remote link in the `remote_channels()` snapshot.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RemoteChannelEntry {
    pub alias: String,
    pub remote_mcp_nom: String,
    pub address: String,
}

/// Success result of `remote_channels()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RemoteChannelsEnvelope {
    pub channels: Vec<RemoteChannelEntry>,
}

#[mcp::tool_router(router = remote_channels_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "List the open mTLS links to remote grammar hosts.",
        output_schema = mcp::schema_for_type::<RemoteChannelsEnvelope>()
    )]
    pub(crate) async fn remote_channels(
        &self,
        mcp::Parameters(_p): mcp::Parameters<RemoteChannelsParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let links = self.remote_links.lock().unwrap_or_else(|e| e.into_inner());
        let mut channels: Vec<RemoteChannelEntry> = links
            .iter()
            .map(|(alias, entry)| RemoteChannelEntry {
                alias: alias.clone(),
                remote_mcp_nom: entry.handle.remote_mcp_nom.clone(),
                address: entry.addr.to_string(),
            })
            .collect();
        drop(links);
        channels.sort_by(|a, b| a.alias.cmp(&b.alias));
        envelope_to_structured(&RemoteChannelsEnvelope { channels })
    }
}
