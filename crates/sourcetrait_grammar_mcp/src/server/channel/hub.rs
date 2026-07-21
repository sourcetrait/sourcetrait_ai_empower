use crate::*;

/// The hub binds loopback, always, and this is not configurable.
///
/// The channel is a single-host design, so "only localhost gets in" is a property of
/// the socket rather than a policy to enforce - the kernel will not route anything else
/// to a 127.0.0.1 listener, which is why there is no peer allow/deny list anywhere.
/// A literal rather than `localhost` also removes the resolution ambiguity that made a
/// v4 reset read as a TLS verdict during P3; the leaf carries IP SANs for this.
const BIND: &str = "127.0.0.1";

/// The origin stamped on a host-originated packet. Plain `mcp`, not `mcp/<id>`: the
/// channel is 1:1, so there is no second MCP to tell apart.
pub(crate) const FROM_MCP: &str = "mcp";

/// A CORE model drops the vendor prefix; everything else carries its own.
pub(crate) const MODEL_OPEN: &str = "channel/Open";

/// RFC 6455 "try again later"; the plan's refusal code for a second claimant.
const CLOSE_CLAIMED: u16 = 1013;
const CLAIM_REASON: &str = "channel already claimed";

/// The host's `channel/Open` control packet - the thing the agent verifies by SEEING.
///
/// Built in one place because it has two emitters: the hub greets a freshly connected
/// peer with it, and `channel_open` re-sends it on an EXISTING channel, where a failing
/// send is what reveals a peer that has actually gone.
pub(crate) fn open_packet(
    nonce_gen: &NonceGen,
    mcp_nom: McpNom,
) -> Result<String, String> {
    let span = nu::Span::unknown();
    let mut data = nu::Record::new();
    data.insert("mcp_nom", nu::Value::string(mcp_nom.to_string(), span));
    let event = nu::Value::record(data, span);
    let event_nuon = render_nuon(&event)?;
    let id = mint_msg_id(nonce_gen, FROM_MCP, MODEL_OPEN, &event_nuon, None);
    render_packet(id, FROM_MCP, MODEL_OPEN, &event, None)
}

fn server_config() -> Result<Arc<tls::ServerConfig>, Error> {
    let (leaf_path, key_path) = config().channel.cert_paths();
    let leaf = tls::CertificateDer::from_pem_file(&leaf_path).map_err(|e| Error::ChannelStart {
        reason: format!("read {}: {e}", leaf_path.display()),
    })?;
    let key = tls::PrivateKeyDer::from_pem_file(&key_path).map_err(|e| Error::ChannelStart {
        reason: format!("read {}: {e}", key_path.display()),
    })?;
    // Explicit provider rather than the process-global: rustls is pinned to ring (its
    // own default is aws_lc_rs), and passing it by hand keeps the hub independent of
    // whatever else may have installed a default.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = tls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_no_client_auth().with_single_cert(vec![leaf], key))
        .map_err(|e| Error::ChannelStart {
            reason: format!("server config: {e}"),
        })?;
    Ok(Arc::new(config))
}

/// Bind the hub and hand back the URL to advertise.
///
/// EPHEMERAL PORT: the agent always learns the endpoint from `channel_open`'s return, so
/// a fixed port buys nothing and costs a squatting failure mode - a stale host or an
/// escaped child holding it makes the next open fail with a raw `Address already in use`
/// (hit live during P3).
pub(crate) async fn start(
    handle: &ChannelHandle,
    mcp_nom: McpNom,
    nonce_gen: Arc<NonceGen>,
) -> Result<String, Error> {
    let tls_config = server_config()?;
    // 0 is the SYSCALL's "assign me one" convention, not a configurable value - the
    // config models an unpinned port as None, and this is the one place it becomes 0.
    let listener = tk::TcpListener::bind((BIND, config().channel.port.unwrap_or(0)))
        .await
        .map_err(|e| Error::ChannelStart {
            reason: format!("bind {BIND}: {e}"),
        })?;
    // Read the port back rather than trusting the configured value: with 0 it is the
    // only way to know it, and with a pinned one it confirms what we actually got.
    let port = listener
        .local_addr()
        .map_err(|e| Error::ChannelStart {
            reason: format!("local_addr: {e}"),
        })?
        .port();
    let url = format!("wss://{BIND}:{port}");
    let (packet_tx, packet_rx) = tk::unbounded_channel::<String>();
    let (close_tx, close_rx) = tk::oneshot::channel::<(u16, String)>();
    let (shutdown_tx, shutdown_rx) = tk::oneshot::channel::<()>();
    let claimed = Arc::new(AtomicBool::new(false));
    tk::spawn(accept_loop(
        listener,
        tls::TlsAcceptor::from(tls_config),
        packet_rx,
        close_rx,
        shutdown_rx,
        claimed.clone(),
        mcp_nom,
        nonce_gen,
    ));
    handle.install(url.clone(), packet_tx, close_tx, shutdown_tx, claimed);
    Ok(url)
}

#[allow(clippy::too_many_arguments)]
async fn accept_loop(
    listener: tk::TcpListener,
    acceptor: tls::TlsAcceptor,
    packet_rx: tk::UnboundedReceiver<String>,
    close_rx: tk::oneshot::Receiver<(u16, String)>,
    shutdown_rx: tk::oneshot::Receiver<()>,
    claimed: Arc<AtomicBool>,
    mcp_nom: McpNom,
    nonce_gen: Arc<NonceGen>,
) {
    let mut peer_slot = Some((packet_rx, close_rx));
    let mut shutdown_rx = shutdown_rx;
    loop {
        tokio::select! {
            // Dropping the sender counts as shutdown, so a torn-down channel ends the
            // loop even though nothing is ever sent on it.
            _ = &mut shutdown_rx => break,
            accepted = listener.accept() => {
                let Ok((tcp, _peer)) = accepted else { continue };
                // compare_exchange, not load-then-store: two simultaneous connections
                // must not both read "unclaimed" and both win.
                let won = claimed
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok();
                let acceptor = acceptor.clone();
                if won {
                    if let Some((packet_rx, close_rx)) = peer_slot.take() {
                        tk::spawn(serve_peer(
                            tcp,
                            acceptor,
                            packet_rx,
                            close_rx,
                            mcp_nom,
                            nonce_gen.clone(),
                        ));
                    }
                } else {
                    tk::spawn(refuse_peer(tcp, acceptor));
                }
            }
        }
    }
}

/// Serve the one claiming peer for the channel's life.
async fn serve_peer(
    tcp: tk::TcpStream,
    acceptor: tls::TlsAcceptor,
    mut packets: tk::UnboundedReceiver<String>,
    close_rx: tk::oneshot::Receiver<(u16, String)>,
    mcp_nom: McpNom,
    nonce_gen: Arc<NonceGen>,
) {
    let Ok(tls_stream) = acceptor.accept(tcp).await else {
        return;
    };
    let Ok(mut socket) = ws::accept_async(tls_stream).await else {
        return;
    };

    // The plan's step-3 verification: the agent proves the channel by SEEING a real
    // packet, so the host emits one the moment a peer connects. There is no separate
    // ack channel by design.
    if let Ok(line) = open_packet(&nonce_gen, mcp_nom)
        && socket.send(ws::Message::text(line)).await.is_err()
    {
        return;
    }

    let mut close_rx = close_rx;
    loop {
        tokio::select! {
            // BIASED, close arm FIRST. `close_locked` signals the close and DROPS the
            // packet sender in the same breath, so this arm and the `None` arm below go
            // ready together - and an unbiased select picks a ready arm at RANDOM. That
            // made a planned close reach the peer as our 1000 only about half the time,
            // otherwise breaking the loop with no frame at all and leaving a bare 1006
            // the agent cannot tell from a crashed host. Polling the close first is also
            // the design stated directly: it must never wait behind queued traffic.
            biased;

            closing = &mut close_rx => {
                if let Ok((code, reason)) = closing {
                    let frame = ws::CloseFrame {
                        code: ws::CloseCode::from(code),
                        reason: reason.into(),
                    };
                    let _ = socket.send(ws::Message::Close(Some(frame))).await;
                    let _ = socket.flush().await;
                }
                break;
            }
            line = packets.recv() => match line {
                Some(line) => {
                    // The last point before the wire, so the guard lives here: an
                    // oversize frame is dropped whole by the client and a cap-exact one
                    // arrives corrupted, so neither may be written (P8).
                    if line.len() >= MAX_FRAME_BYTES {
                        eprintln!(
                            "grammar: channel packet of {} bytes refused (max {})",
                            line.len(),
                            MAX_FRAME_BYTES,
                        );
                        continue;
                    }
                    if socket.send(ws::Message::text(line)).await.is_err() {
                        break;
                    }
                }
                // The handle went away with no close to announce.
                None => break,
            },
            incoming = socket.next() => match incoming {
                // Polling the read half is not optional: it is what lets tungstenite
                // answer pings and observe the peer's own close.
                None | Some(Ok(ws::Message::Close(_))) | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Refuse a second claimant, saying why.
///
/// P11 verified our close CODE and REASON both reach the client verbatim, so a refusal
/// is legible rather than looking like a crash.
async fn refuse_peer(
    tcp: tk::TcpStream,
    acceptor: tls::TlsAcceptor,
) {
    let Ok(tls_stream) = acceptor.accept(tcp).await else {
        return;
    };
    let Ok(mut socket) = ws::accept_async(tls_stream).await else {
        return;
    };
    let frame = ws::CloseFrame {
        code: ws::CloseCode::from(CLOSE_CLAIMED),
        reason: CLAIM_REASON.to_string().into(),
    };
    let _ = socket.send(ws::Message::Close(Some(frame))).await;
    let _ = socket.flush().await;
}
