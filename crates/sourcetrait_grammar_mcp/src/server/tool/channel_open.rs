use crate::*;

/// Parameters for `channel_open()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ChannelOpenParams {}

/// Success result of `channel_open()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ChannelOpenEnvelope {
    /// `new` when this call started the hub, `existing` when one was already running.
    pub status: String,
    /// The endpoint to point a Monitor at; the port is whatever was actually bound.
    pub wss: String,
    /// The directory an oversized packet spills into.
    pub inbox: String,
}

const STATUS_NEW: &str = "new";
const STATUS_EXISTING: &str = "existing";

/// The env var naming the tmpfs IPC root the inbox lives under.
const SHM_ROOT_VAR: &str = "$XDGX_SHM_DIR";

/// How long a peer has to prove it owns the stdio session. A CODE CONSTANT, not a
/// config option: it is a property of the handshake rather than of a deployment.
const VERIFY_WINDOW: tk::TkDuration = tk::TkDuration::from_secs(300);

/// RFC 6455 policy violation - the peer never verified. A distinct code because close
/// codes and reasons reach the agent verbatim, so the three teardowns (refused 1013,
/// planned 1000, unverified 1008) stay tellable apart.
const CLOSE_UNVERIFIED: u16 = 1008;
const UNVERIFIED_REASON: &str = "verification window expired";

/// `$XDGX_SHM_DIR/mcp/<mcp_nom>/inbox`, created here. Phase 3 fills it; this phase only
/// has to report where it is.
fn ensure_inbox(
    channel: &ChannelHandle,
    mcp_nom: McpNom,
) -> Result<String, Error> {
    let root = expand_path(SHM_ROOT_VAR).map_err(|reason| Error::ChannelStart { reason })?;
    let dir = root
        .join("mcp")
        .join(mcp_nom.to_string())
        .join("inbox");
    fs::create_dir_all(&dir)?;
    // Handing it to the channel is what lets the emit path write an attachment without
    // knowing the namespace.
    channel.set_inbox(dir.clone());
    Ok(dir.display().to_string())
}

/// Re-greet an existing channel's peer. A failing send is the ONLY way to learn the
/// connection has gone, since nothing else reports a peer that simply went away.
fn resend_open_packet(
    channel: &ChannelHandle,
    nonce_gen: &NonceGen,
    mcp_nom: McpNom,
) -> Result<(), Error> {
    let line = open_packet(nonce_gen, mcp_nom).map_err(|reason| Error::Internal {
        phase: "channel_open::render".to_string(),
        reason,
    })?;
    channel.send_control(line).map_err(|e| match e {
        ChannelSendError::NotOpen => Error::ChannelNotOpen,
        _ => Error::ChannelPeerGone,
    })
}

/// Start the verify window. Expiry tears the hub down; verification or a close drops
/// the sender, which stands this task down instead.
fn arm_verify_timer(channel: Arc<ChannelHandle>) {
    let (cancel_tx, cancel_rx) = tk::oneshot::channel::<()>();
    channel.arm_verify(cancel_tx);
    tk::spawn(async move {
        tokio::select! {
            _ = cancel_rx => {}
            _ = tk::sleep(VERIFY_WINDOW) => {
                channel.close_if_unverified(CLOSE_UNVERIFIED, UNVERIFIED_REASON);
            }
        }
    });
}

#[mcp::tool_router(router = channel_open_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Open the host's packet channel and return the endpoint to watch.",
        output_schema = mcp::schema_for_type::<ChannelOpenEnvelope>()
    )]
    pub(crate) async fn channel_open(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ChannelOpenParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let inbox = match ensure_inbox(&self.channel, self.mcp_nom) {
            Ok(path) => path,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        // The decide-then-start sequence spans an await, so it runs under one guard:
        // two concurrent calls would otherwise both read Closed and both bind a hub.
        let _opening = self.channel_open_lock.lock().await;
        let status = self.channel.status();
        let (label, wss) = if matches!(status.phase, ChannelPhase::Closed) {
            match start_channel_hub(&self.channel, self.mcp_nom, self.nonce_gen.clone()).await {
                Ok(url) => (STATUS_NEW, url),
                Err(error) => return Ok(error_to_call_result(error, None)),
            }
        } else {
            let Some(url) = status.url else {
                return Ok(error_to_call_result(
                    Error::Internal {
                        phase: "channel_open::url".to_string(),
                        reason: "an open channel carries no url".to_string(),
                    },
                    None,
                ));
            };
            // Re-send the greeting so a peer that has gone away is DETECTED here, as a
            // failing send, rather than by the agent waiting for packets that will
            // never arrive. Guarded on `claimed` because an unclaimed hub has no
            // connection to test and greets the next Monitor itself - an extra packet
            // there would only queue a duplicate.
            if status.claimed
                && let Err(error) =
                    resend_open_packet(&self.channel, &self.nonce_gen, self.mcp_nom)
            {
                return Ok(error_to_call_result(error, None));
            }
            (STATUS_EXISTING, url)
        };
        // Armed on BOTH paths: an existing channel has to re-prove its peer too, and a
        // fresh arm stands the previous timer down.
        arm_verify_timer(self.channel.clone());
        envelope_to_structured(&ChannelOpenEnvelope {
            status: label.to_string(),
            wss,
            inbox,
        })
    }
}
