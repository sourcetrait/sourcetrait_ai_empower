#![allow(dead_code)]
//! Remote link lifecycle: two entity-pinned mTLS connections to one peer,
//! message/control + file. A link is opened on demand by remote_channel_open
//! (connector or listener, from config), after the agent's Channel is up. The
//! open task owns the lifecycle: it emits mcp/remote/Connected once both
//! connections handshake, drives the link, and emits mcp/remote/Disconnected
//! when it ends. Sends carry Channel packets: the receiver renders + emits them
//! onto its own Channel with from = mcp/remote/<sender_nom>, files alongside.
use crate::*;

/// Server-name presented on the client handshake; entity-pin ignores it.
const PIN_SERVER_NAME: &str = "grammar.invalid";

/// How long a closing side waits for the reciprocal `Close` before giving up.
const CLOSE_FRAME_GRACE: tk::TkDuration = tk::TkDuration::from_secs(2);

/// One file chunk's uncompressed payload ceiling; zstd + the frame ride on top.
const CHUNK_BYTES: usize = 256 * 1024;

/// How long a sender waits for a delivery receipt before reporting Unsent{timeout}.
const SEND_ACK_TIMEOUT: tk::TkDuration = tk::TkDuration::from_secs(30);

/// Host-origin lifecycle models, on the local Channel under the mcp/ reservation.
const MODEL_CONNECTED: &str = "mcp/remote/Connected";
const MODEL_DISCONNECTED: &str = "mcp/remote/Disconnected";

// ---- the process-wide open-link registry ----

/// Every open link, keyed by alias. A std Mutex, not the async one, so a grimm
/// send reaches it from the synchronous eval thread; guards are only held for
/// map ops, never across an await.
static REMOTE_LINKS: LazyLock<Arc<std::sync::Mutex<HashMap<String, RemoteLinkEntry>>>> =
    LazyLock::new(|| Arc::new(std::sync::Mutex::new(HashMap::new())));

/// The shared registry handle; NuSh holds a clone, the grimm sends reach it here.
pub(crate) fn remote_links() -> Arc<std::sync::Mutex<HashMap<String, RemoteLinkEntry>>> {
    REMOTE_LINKS.clone()
}

/// Find the open link to `mcp_nom` and enqueue a send. The lock is held only for
/// the lookup + the synchronous mpsc pushes.
pub(crate) fn find_link_send(
    mcp_nom: &str,
    id: String,
    model: String,
    event_nuon: String,
    payloads: Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let links = remote_links();
    let guard = links.lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard
        .values()
        .find(|e| e.handle.remote_mcp_nom == mcp_nom)
        .ok_or_else(|| format!("no open remote link to mcp_nom `{mcp_nom}`"))?;
    entry.handle.enqueue_send(id, model, event_nuon, payloads)
}

/// A dest is a relative path with no `..` component - never an escape from the
/// per-packet inbox. Enforced sender-side (a clean error) and receiver-side
/// (defence against a buggy peer).
pub(crate) fn safe_dest(dest: &str) -> bool {
    let path = std::path::Path::new(dest);
    !dest.is_empty()
        && path.is_relative()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Open the link for a configured entry: spawn its lifecycle task and return at
/// once. remote_channel_open drives this; the Connected notification lands async.
pub(crate) fn open_remote(
    self_mcp_nom: String,
    entry: RemoteEntry,
) {
    let addr_hint = match &entry.role {
        RemoteRole::Connector { addr } => *addr,
        RemoteRole::Listener { bind, .. } => *bind,
    };
    tk::spawn(async move {
        let (established, fail_kind) = match entry.role.clone() {
            RemoteRole::Connector { addr } => {
                (connect_link(&self_mcp_nom, &entry, addr).await, "connect")
            }
            RemoteRole::Listener { bind, allow } => {
                (listen_link(&self_mcp_nom, &entry, bind, allow).await, "listen")
            }
        };
        let (handle, msg_join, file_join) = match established {
            Ok(v) => v,
            Err(e) => {
                emit_open_failed(&entry.alias, fail_kind, &e.to_string());
                return;
            }
        };
        let peer_nom = handle.remote_mcp_nom.clone();
        remote_links()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(entry.alias.clone(), RemoteLinkEntry { handle, addr: addr_hint });
        emit_lifecycle(MODEL_CONNECTED, &peer_nom);
        let _ = msg_join.await;
        let _ = file_join.await;
        // Deregister only if this link is still the one registered - a re-open
        // may have replaced it under the same alias.
        {
            let links_arc = remote_links();
            let mut links = links_arc.lock().unwrap_or_else(|e| e.into_inner());
            if links
                .get(&entry.alias)
                .is_some_and(|e| e.handle.remote_mcp_nom == peer_nom)
            {
                links.remove(&entry.alias);
            }
        }
        emit_lifecycle(MODEL_DISCONNECTED, &peer_nom);
    });
}

/// Emit a host-origin lifecycle packet {mcp_nom: peer} onto the local Channel.
fn emit_lifecycle(
    model: &str,
    peer_nom: &str,
) {
    let span = nu::Span::unknown();
    let mut event = nu::Record::new();
    event.insert("mcp_nom", nu::Value::string(peer_nom.to_string(), span));
    push_report(model, nu::Value::record(event, span));
}

/// Emit mcp/remote/Disconnected {alias, error} when a link never established.
/// No peer McpNom exists (the handshake never completed), so the alias the agent
/// opened is the identifier the report carries in its place.
fn emit_open_failed(
    alias: &str,
    kind: &str,
    message: &str,
) {
    let span = nu::Span::unknown();
    let mut err = nu::Record::new();
    err.insert("kind", nu::Value::string(kind.to_string(), span));
    err.insert("message", nu::Value::string(message.to_string(), span));
    let mut event = nu::Record::new();
    event.insert("alias", nu::Value::string(alias.to_string(), span));
    event.insert("error", nu::Value::record(err, span));
    push_report(MODEL_DISCONNECTED, nu::Value::record(event, span));
}

/// What connecting to a remote acceptor needs (a resolved connector entry).
pub(crate) struct RemoteLinkOptions {
    /// The acceptor's listen address.
    pub addr: std::net::SocketAddr,
    /// This host's own entity leaf, presented as the client certificate.
    pub self_cert_file: PathBuf,
    /// The private key paired with `self_cert_file`.
    pub self_key_file: PathBuf,
    /// The peer's entity leaf; the presented leaf must match it byte-for-byte.
    pub peer_pin_file: PathBuf,
}

/// A running link: the peer McpNom, the outbound queues a send pushes onto, and
/// the shared cancel. The join handles live in the open task, not here, so
/// cancelling (remote_channel_close) lets that task observe the end and report.
pub(crate) struct RemoteLinkHandle {
    pub remote_mcp_nom: String,
    cancel: tku::CancellationToken,
    /// Deliver/receipt to the message-connection driver.
    msg_out_tx: tk::UnboundedSender<MsgFrame>,
    /// FileChunk to the file-connection driver.
    file_out_tx: tk::UnboundedSender<FileFrame>,
}

impl RemoteLinkHandle {
    /// Cancel both drivers; the open task awaits their joins, deregisters, and
    /// emits Disconnected.
    pub(crate) fn cancel(&self) {
        if !self.cancel.is_cancelled() {
            self.cancel.cancel();
        }
    }

    /// Chunk each file onto the file connection, then Deliver onto the message
    /// connection. Synchronous (mpsc sends), so a grimm eval thread drives it; a
    /// closed receiver (driver gone) surfaces as an error the send reports.
    pub(crate) fn enqueue_send(
        &self,
        id: String,
        model: String,
        event_nuon: String,
        payloads: Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        let closing = || "remote link is closing".to_string();
        let dests: Vec<String> = payloads.iter().map(|(dest, _)| dest.clone()).collect();
        for (dest, bytes) in payloads {
            if bytes.is_empty() {
                self.file_out_tx
                    .send(FileFrame::Chunk {
                        id: id.clone(),
                        dest: dest.clone(),
                        seq: 0,
                        bytes: Vec::new(),
                        last: true,
                    })
                    .map_err(|_| closing())?;
                continue;
            }
            let total = bytes.len().div_ceil(CHUNK_BYTES);
            for (i, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
                self.file_out_tx
                    .send(FileFrame::Chunk {
                        id: id.clone(),
                        dest: dest.clone(),
                        seq: i as u32,
                        bytes: chunk.to_vec(),
                        last: i + 1 == total,
                    })
                    .map_err(|_| closing())?;
            }
        }
        self.msg_out_tx
            .send(MsgFrame::Deliver {
                id,
                model,
                event_nuon,
                files: dests,
            })
            .map_err(|_| closing())
    }
}

/// The drivers a spawn returns: the handle plus both join handles (the open task
/// awaits the joins to learn when the link has ended).
type LinkDrivers = (RemoteLinkHandle, tk::JoinHandle<()>, tk::JoinHandle<()>);

/// Connect as the initiator: two mTLS connections (message + file), each
/// handshaked, both drivers spawned under one cancel.
async fn connect_link(
    self_mcp_nom: &str,
    entry: &RemoteEntry,
    addr: std::net::SocketAddr,
) -> io::Result<LinkDrivers> {
    let opts = RemoteLinkOptions {
        addr,
        self_cert_file: entry.self_cert_file.clone(),
        self_key_file: entry.self_key_file.clone(),
        peer_pin_file: entry.peer_pin_file.clone(),
    };
    let connector = tls::TlsConnector::from(client_config(&opts)?);
    let (remote_msg, msg_read, msg_write) =
        connect_conn(&connector, addr, self_mcp_nom, RemoteStream::Message).await?;
    let (remote_file, file_read, file_write) =
        connect_conn(&connector, addr, self_mcp_nom, RemoteStream::File).await?;
    pair_check(&remote_msg, &remote_file)?;
    Ok(spawn_initiator_drivers(
        self_mcp_nom,
        remote_msg,
        (msg_read, msg_write),
        (file_read, file_write),
    ))
}

/// Listen as the acceptor: bind, take one peer's two connections (entity-pinned,
/// optionally source-IP-filtered), spawn its drivers. Binding stops once paired.
async fn listen_link(
    self_mcp_nom: &str,
    entry: &RemoteEntry,
    bind: std::net::SocketAddr,
    allow: Option<std::net::IpAddr>,
) -> io::Result<LinkDrivers> {
    let tls_config = server_config(&entry.self_cert_file, &entry.self_key_file, &entry.peer_pin_file)?;
    let acceptor = tls::TlsAcceptor::from(tls_config);
    let listener = tk::TcpListener::bind(bind).await?;
    let mut message: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
    let mut file: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
    while message.is_none() || file.is_none() {
        let (tcp, peer) = listener.accept().await?;
        if let Some(allow_ip) = allow
            && peer.ip() != allow_ip
        {
            eprintln!(
                "grammar: remote listener `{}` refuses source {} (allows {allow_ip} only)",
                entry.alias,
                peer.ip(),
            );
            continue;
        }
        let (remote, stream, framed_read, framed_write) =
            accept_conn(&acceptor, self_mcp_nom, tcp).await?;
        match stream {
            RemoteStream::Message => message = Some((remote, framed_read, framed_write)),
            RemoteStream::File => file = Some((remote, framed_read, framed_write)),
        }
    }
    let (remote_msg, msg_read, msg_write) = message.expect("message half present");
    let (remote_file, file_read, file_write) = file.expect("file half present");
    pair_check(&remote_msg, &remote_file)?;
    Ok(spawn_acceptor_drivers(
        self_mcp_nom,
        remote_msg,
        (msg_read, msg_write),
        (file_read, file_write),
    ))
}

/// Turn two handshaked initiator connections into a running link + its joins.
fn spawn_initiator_drivers(
    self_mcp_nom: &str,
    remote_mcp_nom: String,
    message: (InitFramedRead, InitFramedWrite),
    file: (InitFramedRead, InitFramedWrite),
) -> LinkDrivers {
    let cancel = tku::CancellationToken::new();
    let (msg_out_tx, msg_out_rx) = tk::unbounded_channel::<MsgFrame>();
    let (file_out_tx, file_out_rx) = tk::unbounded_channel::<FileFrame>();
    let recv = RecvContext::new(self_mcp_nom, &remote_mcp_nom, msg_out_tx.clone());
    let send = SendContext::new(&remote_mcp_nom);
    let message_join = tk::spawn(run_initiator_message(
        cancel.clone(),
        message.0,
        message.1,
        msg_out_rx,
        recv.clone(),
        send,
    ));
    let file_join = tk::spawn(run_initiator_file(cancel.clone(), file.0, file.1, file_out_rx, recv));
    (
        RemoteLinkHandle {
            remote_mcp_nom,
            cancel,
            msg_out_tx,
            file_out_tx,
        },
        message_join,
        file_join,
    )
}

/// Turn two handshaked acceptor connections into a running link + its joins.
fn spawn_acceptor_drivers(
    self_mcp_nom: &str,
    remote_mcp_nom: String,
    message: (AcceptFramedRead, AcceptFramedWrite),
    file: (AcceptFramedRead, AcceptFramedWrite),
) -> LinkDrivers {
    let cancel = tku::CancellationToken::new();
    let (msg_out_tx, msg_out_rx) = tk::unbounded_channel::<MsgFrame>();
    let (file_out_tx, file_out_rx) = tk::unbounded_channel::<FileFrame>();
    let recv = RecvContext::new(self_mcp_nom, &remote_mcp_nom, msg_out_tx.clone());
    let send = SendContext::new(&remote_mcp_nom);
    let message_join = tk::spawn(run_acceptor_message(
        cancel.clone(),
        message.0,
        message.1,
        msg_out_rx,
        recv.clone(),
        send,
    ));
    let file_join = tk::spawn(run_acceptor_file(cancel.clone(), file.0, file.1, file_out_rx, recv));
    (
        RemoteLinkHandle {
            remote_mcp_nom,
            cancel,
            msg_out_tx,
            file_out_tx,
        },
        message_join,
        file_join,
    )
}

/// Both connections of one link must report the same peer McpNom.
fn pair_check(
    message: &str,
    file: &str,
) -> io::Result<()> {
    if message == file {
        Ok(())
    } else {
        Err(io_other(format!(
            "link peers disagree: message conn `{message}`, file conn `{file}`"
        )))
    }
}

// ---- receive + send coordination (peer-neutral, shared by both roles) ----

/// One inbound delivery's state, coordinating its message + file connections.
#[derive(Default)]
struct DeliverSlot {
    /// Set when the Deliver frame arrives.
    deliver: Option<DeliverInfo>,
    /// The manifest dests, set with the Deliver; None until it arrives.
    expected: Option<HashSet<String>>,
    /// Dests whose last chunk has landed.
    done: HashSet<String>,
}

/// The message half of a Deliver: what the relayed packet carries.
struct DeliverInfo {
    model: String,
    event_nuon: String,
}

/// What both drivers of a link share to land + relay an inbound delivery.
#[derive(Clone)]
struct RecvContext {
    /// Per-id slots, coordinating the message + file connections.
    slots: Arc<std::sync::Mutex<HashMap<String, DeliverSlot>>>,
    /// The peer's McpNom - the `from = mcp/remote/<nom>` a relayed packet gets.
    peer_nom: String,
    /// `<shm>/mcp/<self_nom>/inbox` - where landed files + the Channel inbox meet.
    inbox_root: PathBuf,
    /// Push a delivery receipt back onto the message connection.
    msg_out_tx: tk::UnboundedSender<MsgFrame>,
}

impl RecvContext {
    fn new(
        self_mcp_nom: &str,
        peer_nom: &str,
        msg_out_tx: tk::UnboundedSender<MsgFrame>,
    ) -> Self {
        let inbox_root = inbox_dir(self_mcp_nom).unwrap_or_else(|e| {
            eprintln!("grammar: remote inbox root unresolved ({e}); files will not land");
            PathBuf::from("/nonexistent")
        });
        Self {
            slots: Arc::new(std::sync::Mutex::new(HashMap::new())),
            peer_nom: peer_nom.to_string(),
            inbox_root,
            msg_out_tx,
        }
    }
}

/// The sender half: which of this host's sends are awaiting a receipt.
#[derive(Clone)]
struct SendContext {
    pending: Arc<std::sync::Mutex<HashSet<String>>>,
    /// The peer's McpNom, stamped into the Sent/Unsent report event.
    peer_nom: String,
}

impl SendContext {
    fn new(peer_nom: &str) -> Self {
        Self {
            pending: Arc::new(std::sync::Mutex::new(HashSet::new())),
            peer_nom: peer_nom.to_string(),
        }
    }
}

/// Record the Deliver half of a delivery, then try to finalize.
fn record_deliver(
    ctx: &RecvContext,
    id: &str,
    info: DeliverInfo,
    files: Vec<String>,
) {
    {
        let mut map = ctx.slots.lock().unwrap_or_else(|e| e.into_inner());
        let slot = map.entry(id.to_string()).or_default();
        slot.deliver = Some(info);
        slot.expected = Some(files.into_iter().collect());
    }
    try_finalize(ctx, id);
}

/// Record that one dest's last chunk landed, then try to finalize.
fn record_file_done(
    ctx: &RecvContext,
    id: &str,
    dest: &str,
) {
    {
        let mut map = ctx.slots.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(id.to_string())
            .or_default()
            .done
            .insert(dest.to_string());
    }
    try_finalize(ctx, id);
}

/// If the Deliver is in and every manifest file has landed, relay once and ack.
/// Called by whichever driver completed the condition; the map remove makes it
/// fire exactly once.
fn try_finalize(
    ctx: &RecvContext,
    id: &str,
) {
    let taken = {
        let mut map = ctx.slots.lock().unwrap_or_else(|e| e.into_inner());
        let ready = map.get(id).is_some_and(|slot| {
            slot.deliver.is_some()
                && slot
                    .expected
                    .as_ref()
                    .is_some_and(|want| want.is_subset(&slot.done))
        });
        if ready { map.remove(id) } else { None }
    };
    let Some(slot) = taken else { return };
    let info = slot.deliver.expect("ready implies a deliver");
    let has_files = slot.expected.is_some_and(|want| !want.is_empty());
    let result = match relay_to_channel(ctx, id, &info, has_files) {
        Ok(()) => DeliveryResult::Accepted,
        Err((kind, message)) => DeliveryResult::Refused { kind, message },
    };
    let _ = ctx.msg_out_tx.send(MsgFrame::DeliverAck {
        id: id.to_string(),
        result,
    });
}

/// Append one chunk to `<inbox_root>/<peer_nom>/<id>/<dest>`; true iff `last`.
fn land_file_chunk(
    ctx: &RecvContext,
    id: &str,
    dest: &str,
    seq: u32,
    bytes: &[u8],
    last: bool,
) -> io::Result<bool> {
    let path = ctx.inbox_root.join(&ctx.peer_nom).join(id).join(dest);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = if seq == 0 {
        fs::File::create(&path)?
    } else {
        fs::OpenOptions::new().append(true).open(&path)?
    };
    file.write_all(bytes)?;
    Ok(last)
}

/// Relay the packet for an inbound delivery onto the local Channel - through
/// `emit`, so it reaches only a verified peer (a remote link comes up after the
/// agent verifies its Channel, so this is Verified in the normal case; if not,
/// the refusal becomes the receipt the sender reports as Unsent).
fn relay_to_channel(
    ctx: &RecvContext,
    id: &str,
    info: &DeliverInfo,
    has_files: bool,
) -> Result<(), (String, String)> {
    let event = nu::from_nuon(&info.event_nuon, None)
        .map_err(|e| ("event_unparseable".to_string(), e.to_string()))?;
    let from = format!("{MCP_RESERVED_PREFIX}remote/{}", ctx.peer_nom);
    let attached = has_files.then(|| format!("{}/{}", ctx.peer_nom, id));
    let channel = channel_handle();
    let msg_id = mint_msg_id(
        channel.nonce_gen(),
        &from,
        &info.model,
        &info.event_nuon,
        attached.as_deref(),
    );
    let line = render_packet(msg_id, &from, &info.model, &event, attached.as_deref())
        .map_err(|e| ("render".to_string(), e))?;
    channel
        .emit(line)
        .map_err(|e| ("channel".to_string(), e.message().to_string()))
}

/// Push a Sent/Unsent delivery report onto the LOCAL (sender's own) Channel.
fn report_delivery(
    peer_nom: &str,
    id: &str,
    result: &DeliveryResult,
) {
    let span = nu::Span::unknown();
    let mut event = nu::Record::new();
    event.insert("id", nu::Value::string(id.to_string(), span));
    event.insert("mcp_nom", nu::Value::string(peer_nom.to_string(), span));
    let model = match result {
        DeliveryResult::Accepted => "mcp/remote/Sent",
        DeliveryResult::Refused { kind, message } => {
            let mut err = nu::Record::new();
            err.insert("kind", nu::Value::string(kind.clone(), span));
            err.insert("message", nu::Value::string(message.clone(), span));
            event.insert("error", nu::Value::record(err, span));
            "mcp/remote/Unsent"
        }
    };
    push_report(model, nu::Value::record(event, span));
}

/// The Unsent report for a send that never got its receipt in time.
fn report_timeout(
    peer_nom: &str,
    id: &str,
) {
    report_delivery(
        peer_nom,
        id,
        &DeliveryResult::Refused {
            kind: "timeout".to_string(),
            message: format!("no delivery receipt within {}s", SEND_ACK_TIMEOUT.as_secs()),
        },
    );
}

/// Mint, render, and emit a host-origin packet (`from = mcp`) onto the Channel.
/// Best effort: the local agent is running (it drove the send / open), so its
/// Channel is Verified; a dropped packet on a since-closed channel is fine.
fn push_report(
    model: &str,
    event: nu::Value,
) {
    let channel = channel_handle();
    let Ok(event_nuon) = render_nuon(&event) else {
        return;
    };
    let msg_id = mint_msg_id(channel.nonce_gen(), FROM_MCP, model, &event_nuon, None);
    if let Ok(line) = render_packet(msg_id, FROM_MCP, model, &event, None) {
        let _ = channel.emit(line);
    }
}

/// Register a send as awaiting its receipt and arm the Unsent{timeout} fallback.
fn arm_send(
    send: &SendContext,
    id: &str,
) {
    {
        let mut pending = send.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.insert(id.to_string());
    }
    let pending = send.pending.clone();
    let peer = send.peer_nom.clone();
    let id = id.to_string();
    tk::spawn(async move {
        tk::sleep(SEND_ACK_TIMEOUT).await;
        let fired = {
            let mut g = pending.lock().unwrap_or_else(|e| e.into_inner());
            g.remove(&id)
        };
        if fired {
            report_timeout(&peer, &id);
        }
    });
}

/// Handle one inbound message-connection frame (receiver + sender sides).
fn handle_msg_frame(
    recv: &RecvContext,
    send: &SendContext,
    frame: MsgFrame,
) {
    match frame {
        MsgFrame::Deliver {
            id,
            model,
            event_nuon,
            files,
        } => record_deliver(recv, &id, DeliverInfo { model, event_nuon }, files),
        MsgFrame::DeliverAck { id, result } => {
            let claimed = send
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            if claimed {
                report_delivery(&send.peer_nom, &id, &result);
            }
        }
    }
}

/// Handle one inbound file-connection frame (receiver side).
fn handle_file_frame(
    recv: &RecvContext,
    frame: FileFrame,
) {
    let FileFrame::Chunk {
        id,
        dest,
        seq,
        bytes,
        last,
    } = frame;
    if !safe_dest(&dest) {
        eprintln!("grammar: remote file chunk for `{id}` has unsafe dest `{dest}`; dropped");
        return;
    }
    match land_file_chunk(recv, &id, &dest, seq, &bytes, last) {
        Ok(true) => record_file_done(recv, &id, &dest),
        Ok(false) => {}
        Err(e) => eprintln!("grammar: remote file chunk write failed for `{id}`/`{dest}`: {e}"),
    }
}

// ---- initiator (TLS client) ----

type InitReadHalf = tk::ReadHalf<tls::client::TlsStream<tk::TcpStream>>;
type InitWriteHalf = tk::WriteHalf<tls::client::TlsStream<tk::TcpStream>>;
type InitFramedRead = tku::FramedRead<InitReadHalf, BitcodeCodec<AcceptorToInitiator>>;
type InitFramedWrite = tku::FramedWrite<InitWriteHalf, BitcodeCodec<InitiatorToAcceptor>>;

/// Connect one mTLS connection and exchange the McpNom handshake.
async fn connect_conn(
    connector: &tls::TlsConnector,
    addr: std::net::SocketAddr,
    self_mcp_nom: &str,
    stream: RemoteStream,
) -> io::Result<(String, InitFramedRead, InitFramedWrite)> {
    let tcp = tk::TcpStream::connect(addr).await?;
    let domain = rv::ServerName::try_from(PIN_SERVER_NAME).map_err(io_other)?;
    let tls = connector.connect(domain, tcp).await?;
    let (read, write) = tk::split(tls);
    let mut framed_read = tku::FramedRead::new(read, BitcodeCodec::<AcceptorToInitiator>::new());
    let mut framed_write = tku::FramedWrite::new(write, BitcodeCodec::<InitiatorToAcceptor>::new());
    framed_write
        .send(InitiatorToAcceptor::Hello {
            mcp_nom: self_mcp_nom.to_string(),
            stream,
        })
        .await?;
    let remote = match framed_read.next().await {
        Some(Ok(AcceptorToInitiator::Hello { mcp_nom })) => mcp_nom,
        Some(Ok(_)) => return Err(io_other("peer sent a non-Hello during handshake")),
        Some(Err(e)) => return Err(e),
        None => return Err(io_other("peer closed during handshake")),
    };
    Ok((remote, framed_read, framed_write))
}

/// Drive the initiator's MESSAGE connection: Deliver/DeliverAck both ways.
async fn run_initiator_message(
    cancel: tku::CancellationToken,
    mut framed_read: InitFramedRead,
    mut framed_write: InitFramedWrite,
    mut msg_out_rx: tk::UnboundedReceiver<MsgFrame>,
    recv: RecvContext,
    send: SendContext,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_initiator(&mut framed_read, &mut framed_write).await;
                break;
            }
            out = msg_out_rx.recv() => match out {
                Some(frame) => {
                    if let MsgFrame::Deliver { id, .. } = &frame {
                        arm_send(&send, id);
                    }
                    if framed_write.send(InitiatorToAcceptor::Msg(frame)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(AcceptorToInitiator::Msg(frame))) => handle_msg_frame(&recv, &send, frame),
                Some(Ok(AcceptorToInitiator::Close)) => {
                    let _ = framed_write.send(InitiatorToAcceptor::Close).await;
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Drive the initiator's FILE connection: FileChunk in (land) and out (send).
async fn run_initiator_file(
    cancel: tku::CancellationToken,
    mut framed_read: InitFramedRead,
    mut framed_write: InitFramedWrite,
    mut file_out_rx: tk::UnboundedReceiver<FileFrame>,
    recv: RecvContext,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_initiator(&mut framed_read, &mut framed_write).await;
                break;
            }
            out = file_out_rx.recv() => match out {
                Some(frame) => {
                    if framed_write.send(InitiatorToAcceptor::File(frame)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(AcceptorToInitiator::File(frame))) => handle_file_frame(&recv, frame),
                Some(Ok(AcceptorToInitiator::Close)) => {
                    let _ = framed_write.send(InitiatorToAcceptor::Close).await;
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Send `Close` and wait briefly for the peer's reciprocal `Close`.
async fn close_initiator(
    framed_read: &mut InitFramedRead,
    framed_write: &mut InitFramedWrite,
) {
    if framed_write.send(InitiatorToAcceptor::Close).await.is_err() {
        return;
    }
    let _ = tk::timeout(CLOSE_FRAME_GRACE, async {
        loop {
            match framed_read.next().await {
                Some(Ok(AcceptorToInitiator::Close)) => break,
                Some(Ok(_)) => continue,
                Some(Err(_)) => break,
                None => break,
            }
        }
    })
    .await;
}

// ---- acceptor (TLS server) ----

type AcceptReadHalf = tk::ReadHalf<tls::server::TlsStream<tk::TcpStream>>;
type AcceptWriteHalf = tk::WriteHalf<tls::server::TlsStream<tk::TcpStream>>;
type AcceptFramedRead = tku::FramedRead<AcceptReadHalf, BitcodeCodec<InitiatorToAcceptor>>;
type AcceptFramedWrite = tku::FramedWrite<AcceptWriteHalf, BitcodeCodec<AcceptorToInitiator>>;

/// Accept one mTLS connection and answer the McpNom handshake.
async fn accept_conn(
    acceptor: &tls::TlsAcceptor,
    self_mcp_nom: &str,
    tcp: tk::TcpStream,
) -> io::Result<(String, RemoteStream, AcceptFramedRead, AcceptFramedWrite)> {
    let tls = acceptor.accept(tcp).await?;
    let (read, write) = tk::split(tls);
    let mut framed_read = tku::FramedRead::new(read, BitcodeCodec::<InitiatorToAcceptor>::new());
    let mut framed_write = tku::FramedWrite::new(write, BitcodeCodec::<AcceptorToInitiator>::new());
    let (remote, stream) = match framed_read.next().await {
        Some(Ok(InitiatorToAcceptor::Hello { mcp_nom, stream })) => (mcp_nom, stream),
        Some(Ok(_)) => return Err(io_other("peer sent a non-Hello during handshake")),
        Some(Err(e)) => return Err(e),
        None => return Err(io_other("peer closed during handshake")),
    };
    framed_write
        .send(AcceptorToInitiator::Hello {
            mcp_nom: self_mcp_nom.to_string(),
        })
        .await?;
    Ok((remote, stream, framed_read, framed_write))
}

/// Drive the acceptor's MESSAGE connection: Deliver/DeliverAck both ways.
async fn run_acceptor_message(
    cancel: tku::CancellationToken,
    mut framed_read: AcceptFramedRead,
    mut framed_write: AcceptFramedWrite,
    mut msg_out_rx: tk::UnboundedReceiver<MsgFrame>,
    recv: RecvContext,
    send: SendContext,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_acceptor(&mut framed_read, &mut framed_write).await;
                break;
            }
            out = msg_out_rx.recv() => match out {
                Some(frame) => {
                    if let MsgFrame::Deliver { id, .. } = &frame {
                        arm_send(&send, id);
                    }
                    if framed_write.send(AcceptorToInitiator::Msg(frame)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(InitiatorToAcceptor::Msg(frame))) => handle_msg_frame(&recv, &send, frame),
                Some(Ok(InitiatorToAcceptor::Close)) => {
                    let _ = framed_write.send(AcceptorToInitiator::Close).await;
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Drive the acceptor's FILE connection: FileChunk in (land) and out (send).
async fn run_acceptor_file(
    cancel: tku::CancellationToken,
    mut framed_read: AcceptFramedRead,
    mut framed_write: AcceptFramedWrite,
    mut file_out_rx: tk::UnboundedReceiver<FileFrame>,
    recv: RecvContext,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_acceptor(&mut framed_read, &mut framed_write).await;
                break;
            }
            out = file_out_rx.recv() => match out {
                Some(frame) => {
                    if framed_write.send(AcceptorToInitiator::File(frame)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(InitiatorToAcceptor::File(frame))) => handle_file_frame(&recv, frame),
                Some(Ok(InitiatorToAcceptor::Close)) => {
                    let _ = framed_write.send(AcceptorToInitiator::Close).await;
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Send `Close` and wait briefly for the peer's reciprocal `Close`.
async fn close_acceptor(
    framed_read: &mut AcceptFramedRead,
    framed_write: &mut AcceptFramedWrite,
) {
    if framed_write.send(AcceptorToInitiator::Close).await.is_err() {
        return;
    }
    let _ = tk::timeout(CLOSE_FRAME_GRACE, async {
        loop {
            match framed_read.next().await {
                Some(Ok(InitiatorToAcceptor::Close)) => break,
                Some(Ok(_)) => continue,
                Some(Err(_)) => break,
                None => break,
            }
        }
    })
    .await;
}

// ---- TLS config (entity-pin mTLS) ----

/// Build the client config: present our leaf, entity-pin the peer's.
fn client_config(opts: &RemoteLinkOptions) -> io::Result<Arc<tls::ClientConfig>> {
    let verifier = Arc::new(EntityPin::new(load_leaf(&opts.peer_pin_file)?));
    let provider = Arc::new(rv::default_provider());
    let config = tls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io_other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(load_chain(&opts.self_cert_file)?, load_key(&opts.self_key_file)?)
        .map_err(io_other)?;
    Ok(Arc::new(config))
}

/// Build the server config: present our leaf, entity-pin the client's.
fn server_config(
    self_cert_file: &std::path::Path,
    self_key_file: &std::path::Path,
    peer_pin_file: &std::path::Path,
) -> io::Result<Arc<tls::ServerConfig>> {
    let verifier = Arc::new(EntityPin::new(load_leaf(peer_pin_file)?));
    let provider = Arc::new(rv::default_provider());
    let config = tls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io_other)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(load_chain(self_cert_file)?, load_key(self_key_file)?)
        .map_err(io_other)?;
    Ok(Arc::new(config))
}

fn load_leaf(path: &std::path::Path) -> io::Result<rv::CertificateDer<'static>> {
    rv::CertificateDer::from_pem_file(path)
        .map_err(|e| io_other(format!("load cert {}: {e}", path.display())))
}

fn load_chain(path: &std::path::Path) -> io::Result<Vec<rv::CertificateDer<'static>>> {
    Ok(vec![load_leaf(path)?])
}

fn load_key(path: &std::path::Path) -> io::Result<tls::PrivateKeyDer<'static>> {
    tls::PrivateKeyDer::from_pem_file(path)
        .map_err(|e| io_other(format!("load key {}: {e}", path.display())))
}

fn io_other<E: Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}
