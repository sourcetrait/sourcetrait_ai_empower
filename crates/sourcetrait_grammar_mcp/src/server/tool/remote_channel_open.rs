use crate::*;

/// Parameters for `remote_channel_open()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RemoteChannelOpenParams {
    /// The registry key, and the `[remote.<alias>]` to use when it is configured.
    pub alias: String,
    /// Peer address `ip:port` (required when `alias` is not configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Our leaf presented to the peer (required when `alias` is not configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_public_key_file: Option<String>,
    /// Our private key (required when `alias` is not configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_private_key_file: Option<String>,
    /// The peer's leaf to pin (required when `alias` is not configured).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_public_key_file: Option<String>,
}

/// Success result of `remote_channel_open()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RemoteChannelOpenEnvelope {
    /// The alias the link is registered under.
    pub alias: String,
    /// The peer's McpNom, learned in the handshake.
    pub remote_mcp_nom: String,
    /// The peer address the link connected to.
    pub address: String,
}

/// Build the link options from a configured alias, or from explicit parameters.
fn resolve_options(p: &RemoteChannelOpenParams) -> Result<RemoteLinkOptions, String> {
    if let Some(cfg) = config().remote.get(&p.alias) {
        return Ok(RemoteLinkOptions {
            addr: cfg.addr,
            self_cert_file: cfg.self_cert_file.clone(),
            self_key_file: cfg.self_key_file.clone(),
            remote_pin_file: cfg.remote_pin_file.clone(),
        });
    }
    let require = |name: &str, value: &Option<String>| -> Result<String, String> {
        value
            .clone()
            .ok_or_else(|| format!("alias `{}` is not configured, so `{name}` is required", p.alias))
    };
    let address = require("address", &p.address)?;
    let addr = address
        .parse::<std::net::SocketAddr>()
        .map_err(|e| format!("address `{address}` is not ip:port: {e}"))?;
    Ok(RemoteLinkOptions {
        addr,
        self_cert_file: expand_path(&require("self_public_key_file", &p.self_public_key_file)?)?,
        self_key_file: expand_path(&require("self_private_key_file", &p.self_private_key_file)?)?,
        remote_pin_file: expand_path(&require("remote_public_key_file", &p.remote_public_key_file)?)?,
    })
}

#[mcp::tool_router(router = remote_channel_open_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Open an mTLS link to a remote grammar host, by alias or explicit keyset.",
        output_schema = mcp::schema_for_type::<RemoteChannelOpenEnvelope>()
    )]
    pub(crate) async fn remote_channel_open(
        &self,
        mcp::Parameters(p): mcp::Parameters<RemoteChannelOpenParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let opts = match resolve_options(&p) {
            Ok(o) => o,
            Err(reason) => {
                return Ok(error_to_call_result(Error::RemoteInvalidParams { reason }, None));
            }
        };
        if self.remote_links.lock().await.contains_key(&p.alias) {
            return Ok(error_to_call_result(
                Error::RemoteAlreadyOpen {
                    alias: p.alias.clone(),
                },
                None,
            ));
        }
        let addr = opts.addr;
        let handle = match RemoteLink::connect(&self.mcp_nom.to_string(), opts).await {
            Ok(handle) => handle,
            Err(e) => {
                return Ok(error_to_call_result(
                    Error::RemoteConnect {
                        alias: p.alias.clone(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        let remote_mcp_nom = handle.remote_mcp_nom.clone();
        self.remote_links
            .lock()
            .await
            .insert(p.alias.clone(), RemoteLinkEntry { handle, addr });
        envelope_to_structured(&RemoteChannelOpenEnvelope {
            alias: p.alias,
            remote_mcp_nom,
            address: addr.to_string(),
        })
    }
}
