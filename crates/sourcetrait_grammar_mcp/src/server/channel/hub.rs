use crate::*;

/// The hub binds loopback, always, and this is not configurable.
///
/// The channel is a single-host design, so "only localhost gets in" is a property of
/// the socket rather than a policy to enforce - the kernel will not route anything else
/// to a 127.0.0.1 listener, which is why there is no peer allow/deny list anywhere.
/// A literal rather than `localhost` also removes the resolution ambiguity that made a
/// v4 reset read as a TLS verdict during P3; the leaf carries IP SANs for this.
const BIND: &str = "127.0.0.1";

const HOST_FROM: &str = "host";
const KIND_CHANNEL_OPEN: &str = "ChannelOpen";

/// RFC 6455 "try again later"; the plan's refusal code for a second claimant.
const CLOSE_CLAIMED: u16 = 1013;
const CLAIM_REASON: &str = "channel already claimed";

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
    let (tx, rx) = tk::unbounded_channel::<HubCommand>();
    let (shutdown_tx, shutdown_rx) = tk::oneshot::channel::<()>();
    let claimed = Arc::new(AtomicBool::new(false));
    tk::spawn(accept_loop(
        listener,
        tls::TlsAcceptor::from(tls_config),
        rx,
        shutdown_rx,
        claimed.clone(),
        mcp_nom,
    ));
    handle.install(url.clone(), tx, shutdown_tx, claimed);
    Ok(url)
}

async fn accept_loop(
    listener: tk::TcpListener,
    acceptor: tls::TlsAcceptor,
    rx: tk::UnboundedReceiver<HubCommand>,
    shutdown_rx: tk::oneshot::Receiver<()>,
    claimed: Arc<AtomicBool>,
    mcp_nom: McpNom,
) {
    let mut rx_slot = Some(rx);
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
                    if let Some(rx) = rx_slot.take() {
                        tk::spawn(serve_peer(tcp, acceptor, rx, mcp_nom));
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
    mut rx: tk::UnboundedReceiver<HubCommand>,
    mcp_nom: McpNom,
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
    let span = nu::Span::unknown();
    let mut data = nu::Record::new();
    data.insert("mcp_nom", nu::Value::string(mcp_nom.to_string(), span));
    let opened = render_packet(
        &mcp_nom.to_string(),
        HOST_FROM,
        KIND_CHANNEL_OPEN,
        nu::Value::record(data, span),
    );
    if let Ok(line) = opened
        && socket.send(ws::Message::text(line)).await.is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            cmd = rx.recv() => match cmd {
                Some(HubCommand::Packet(line)) => {
                    if socket.send(ws::Message::text(line)).await.is_err() {
                        break;
                    }
                }
                Some(HubCommand::Close { code, reason }) => {
                    let frame = ws::CloseFrame {
                        code: ws::CloseCode::from(code),
                        reason: reason.into(),
                    };
                    let _ = socket.send(ws::Message::Close(Some(frame))).await;
                    let _ = socket.flush().await;
                    break;
                }
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
