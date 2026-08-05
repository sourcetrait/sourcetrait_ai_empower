#![allow(dead_code)]
//! Remote link lifecycle: two known-public-key mTLS connections to one peer,
//! message/control + file. A link is opened on demand by remote_channel_open
//! (connector or listener, from config), after the agent's Channel is up. The
//! open task owns the lifecycle: it emits mcp/remote/Connected once both
//! connections handshake, drives the link, and emits mcp/remote/Disconnected
//! when it ends. Sends carry Channel packets: the receiver renders + emits them
//! onto its own Channel with from = mcp/remote/<sender_nom>, files alongside.
use crate::*;

/// Server-name presented on the client handshake; the known-public-key match
/// ignores it.
const PLACEHOLDER_SERVER_NAME: &str = "grammar.invalid";

/// How long a closing side waits for the reciprocal `Close` before giving up.
const CLOSE_FRAME_GRACE: tk::TkDuration = tk::TkDuration::from_secs(2);

/// One file chunk's uncompressed payload ceiling; zstd + the frame ride on top.
const CHUNK_BYTES: usize = 256 * 1024;

/// Host-origin lifecycle + delivery models, on the local Channel under the mcp/
/// reservation.
const MODEL_CONNECTED: &str = "mcp/remote/Connected";
const MODEL_DISCONNECTED: &str = "mcp/remote/Disconnected";
const MODEL_SENT: &str = "mcp/remote/Sent";
const MODEL_UNSENT: &str = "mcp/remote/Unsent";

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

/// The connector's blocking-connect timeout: the dial must land within this or
/// the open returns a synchronous timeout error.
const CONNECT_TIMEOUT: tk::TkDuration = tk::TkDuration::from_secs(20);

/// Open a configured link, BLOCKING on its immediate networking result - bind
/// for a listener, connect for a connector - so remote_channel_open returns
/// synchronously. Only the listener's peer-wait stays asynchronous.
pub(crate) async fn open_remote_blocking(
    self_mcp_nom: String,
    entry: RemoteConfig,
) -> Result<(), String> {
    match entry.role.clone() {
        RemoteRole::Connector { addr } => open_connector(self_mcp_nom, entry, addr).await,
        RemoteRole::Listener { bind, allow } => {
            open_listener(self_mcp_nom, entry, bind, allow).await
        }
    }
}

/// Connector: block on the connect (20s), register the established link, then
/// watch for its teardown asynchronously. No Connected for the open attempt (the
/// synchronous success is the notice); Disconnected still fires on a later drop.
async fn open_connector(
    self_mcp_nom: String,
    entry: RemoteConfig,
    addr: std::net::SocketAddr,
) -> Result<(), String> {
    let (handle, msg_join, file_join) =
        match tk::timeout(CONNECT_TIMEOUT, connect_link(&self_mcp_nom, &entry, addr)).await {
            Ok(Ok(drivers)) => drivers,
            Ok(Err(e)) => return Err(format!("connect {addr}: {e}")),
            Err(_) => {
                return Err(format!(
                    "connect {addr} timed out after {}s",
                    CONNECT_TIMEOUT.as_secs(),
                ));
            }
        };
    let peer_nom = handle.remote_mcp_nom.clone();
    let alias = entry.alias.clone();
    remote_links()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(alias.clone(), RemoteLinkEntry { handle, addr });
    tk::spawn(async move {
        let _ = msg_join.await;
        let _ = file_join.await;
        deregister_if_ours(&alias, &peer_nom);
        emit_lifecycle(MODEL_DISCONNECTED, &alias, &peer_nom);
    });
    Ok(())
}

/// Listener: block on the BIND, then accept + serve asynchronously. The bind
/// result returns synchronously (bound, or the bind error); Connected fires when
/// a peer pairs, Disconnected when the paired link later drops.
async fn open_listener(
    self_mcp_nom: String,
    entry: RemoteConfig,
    bind: std::net::SocketAddr,
    allow: Option<std::net::IpAddr>,
) -> Result<(), String> {
    let (listener, acceptor) = bind_listener(&entry, bind)
        .await
        .map_err(|e| format!("bind {bind}: {e}"))?;
    let alias = entry.alias.clone();
    tk::spawn(async move {
        match accept_and_pair(&self_mcp_nom, listener, acceptor, allow).await {
            Ok((handle, msg_join, file_join)) => {
                let peer_nom = handle.remote_mcp_nom.clone();
                remote_links()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(alias.clone(), RemoteLinkEntry { handle, addr: bind });
                emit_lifecycle(MODEL_CONNECTED, &alias, &peer_nom);
                let _ = msg_join.await;
                let _ = file_join.await;
                deregister_if_ours(&alias, &peer_nom);
                emit_lifecycle(MODEL_DISCONNECTED, &alias, &peer_nom);
            }
            Err(e) => emit_open_failed(&alias, "listen", &e.to_string()),
        }
    });
    Ok(())
}

/// Drop the link registered under `alias` only if it is still the one this task
/// established - a re-open may have replaced it under the same alias.
fn deregister_if_ours(
    alias: &str,
    peer_nom: &str,
) {
    let links_arc = remote_links();
    let mut links = links_arc.lock().unwrap_or_else(|e| e.into_inner());
    if links
        .get(alias)
        .is_some_and(|e| e.handle.remote_mcp_nom == peer_nom)
    {
        links.remove(alias);
    }
}

/// Emit a host-origin lifecycle packet {remote, mcp_nom} onto the local Channel.
fn emit_lifecycle(
    model: &str,
    remote: &str,
    peer_nom: &str,
) {
    let span = nu::Span::unknown();
    let mut event = nu::Record::new();
    event.insert("remote", nu::Value::string(remote.to_string(), span));
    event.insert("mcp_nom", nu::Value::string(peer_nom.to_string(), span));
    push_report(model, nu::Value::record(event, span));
}

/// Emit mcp/remote/Disconnected {remote, error} when a link never established.
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
    event.insert("remote", nu::Value::string(alias.to_string(), span));
    event.insert("error", nu::Value::record(err, span));
    push_report(MODEL_DISCONNECTED, nu::Value::record(event, span));
}

/// What connecting to a remote acceptor needs (a resolved connector entry).
pub(crate) struct RemoteLinkOptions {
    /// The acceptor's listen address.
    pub addr: std::net::SocketAddr,
    /// This host's own public key, presented as the client certificate.
    pub self_public_key_file: PathBuf,
    /// The private key paired with `self_public_key_file`.
    pub self_private_key_file: PathBuf,
    /// The peer's known public key; the presented leaf must match it byte-for-byte.
    pub peer_public_key_file: PathBuf,
}

/// A running link: the peer McpNom, the outbound queues a send pushes onto, and
/// the shared cancel. The join handles live in the open task, not here, so
/// cancelling (remote_channel_close) lets that task observe the end and report.
pub(crate) struct RemoteLinkHandle {
    pub remote_mcp_nom: String,
    cancel: tku::CancellationToken,
    /// Deliver to the message-connection driver.
    msg_out_tx: tk::UnboundedSender<MsgFrame>,
    /// FileChunk to the file-connection driver.
    file_out_tx: tk::UnboundedSender<FileFrame>,
    /// Aggregate write-outcome tracking behind the Sent/Unsent report.
    send: SendTracker,
}

impl RemoteLinkHandle {
    /// Cancel both drivers; the open task awaits their joins, deregisters, and
    /// emits Disconnected.
    pub(crate) fn cancel(&self) {
        if !self.cancel.is_cancelled() {
            self.cancel.cancel();
        }
    }

    /// Register the send, then queue its frames (file chunks onto the file
    /// connection, the Deliver onto the message connection). Synchronous (mpsc
    /// sends), so a grimm eval thread drives it. Registration precedes the pushes
    /// so no write outcome can arrive for an unknown id; a mid-push failure means
    /// the driver is gone (link down at enqueue) - forget the send and surface a
    /// synchronous error, since nothing left the host and there is no async notice.
    pub(crate) fn enqueue_send(
        &self,
        id: String,
        model: String,
        event_nuon: String,
        payloads: Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        let dests: Vec<String> = payloads.iter().map(|(dest, _)| dest.clone()).collect();
        self.send.register(&id, &dests);
        match push_frames(
            &self.msg_out_tx,
            &self.file_out_tx,
            &id,
            model,
            event_nuon,
            payloads,
            dests,
        ) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.send.forget(&id);
                Err(e)
            }
        }
    }
}

/// Queue every frame of one send: each file's chunks onto the file connection,
/// then the Deliver onto the message connection. A closed receiver (the driver is
/// gone) is the link-down-at-enqueue case, returned as an error.
fn push_frames(
    msg_out_tx: &tk::UnboundedSender<MsgFrame>,
    file_out_tx: &tk::UnboundedSender<FileFrame>,
    id: &str,
    model: String,
    event_nuon: String,
    payloads: Vec<(String, Vec<u8>)>,
    dests: Vec<String>,
) -> Result<(), String> {
    let closing = || "remote link is closing".to_string();
    for (dest, bytes) in payloads {
        if bytes.is_empty() {
            file_out_tx
                .send(FileFrame::Chunk {
                    id: id.to_string(),
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
            file_out_tx
                .send(FileFrame::Chunk {
                    id: id.to_string(),
                    dest: dest.clone(),
                    seq: i as u32,
                    bytes: chunk.to_vec(),
                    last: i + 1 == total,
                })
                .map_err(|_| closing())?;
        }
    }
    msg_out_tx
        .send(MsgFrame::Deliver {
            id: id.to_string(),
            model,
            event_nuon,
            files: dests,
        })
        .map_err(|_| closing())
}

/// The drivers a spawn returns: the handle plus both join handles (the open task
/// awaits the joins to learn when the link has ended).
type LinkDrivers = (RemoteLinkHandle, tk::JoinHandle<()>, tk::JoinHandle<()>);

/// Connect as the initiator: two mTLS connections (message + file), each
/// handshaked, both drivers spawned under one cancel.
async fn connect_link(
    self_mcp_nom: &str,
    entry: &RemoteConfig,
    addr: std::net::SocketAddr,
) -> io::Result<LinkDrivers> {
    let opts = RemoteLinkOptions {
        addr,
        self_public_key_file: entry.self_public_key_file.clone(),
        self_private_key_file: entry.self_private_key_file.clone(),
        peer_public_key_file: entry.peer_public_key_file.clone(),
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

/// Bind the listener socket + build its known-public-key TLS acceptor: the
/// synchronous half of a listener open, so a bind failure surfaces at the tool.
async fn bind_listener(
    entry: &RemoteConfig,
    bind: std::net::SocketAddr,
) -> io::Result<(tk::TcpListener, tls::TlsAcceptor)> {
    let tls_config = server_config(
        &entry.self_public_key_file,
        &entry.self_private_key_file,
        &entry.peer_public_key_file,
    )?;
    let acceptor = tls::TlsAcceptor::from(tls_config);
    let listener = tk::TcpListener::bind(bind).await?;
    Ok((listener, acceptor))
}

/// Accept one peer's two connections (known-public-key-verified, optionally
/// source-IP-filtered) on an already-bound listener + spawn its drivers. The
/// asynchronous half of a listener open; the bind already succeeded.
async fn accept_and_pair(
    self_mcp_nom: &str,
    listener: tk::TcpListener,
    acceptor: tls::TlsAcceptor,
    allow: Option<std::net::IpAddr>,
) -> io::Result<LinkDrivers> {
    let mut message: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
    let mut file: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
    while message.is_none() || file.is_none() {
        let (tcp, peer) = listener.accept().await?;
        if let Some(allow_ip) = allow
            && peer.ip() != allow_ip
        {
            eprintln!(
                "grammar: remote listener refuses source {} (allows {allow_ip} only)",
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
    let recv = RecvContext::new(self_mcp_nom, &remote_mcp_nom);
    let send = SendTracker::new(&remote_mcp_nom);
    let message_join = tk::spawn(run_initiator_message(
        cancel.clone(),
        message.0,
        message.1,
        msg_out_rx,
        recv.clone(),
        send.clone(),
    ));
    let file_join = tk::spawn(run_initiator_file(
        cancel.clone(),
        file.0,
        file.1,
        file_out_rx,
        recv,
        send.clone(),
    ));
    (
        RemoteLinkHandle {
            remote_mcp_nom,
            cancel,
            msg_out_tx,
            file_out_tx,
            send,
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
    let recv = RecvContext::new(self_mcp_nom, &remote_mcp_nom);
    let send = SendTracker::new(&remote_mcp_nom);
    let message_join = tk::spawn(run_acceptor_message(
        cancel.clone(),
        message.0,
        message.1,
        msg_out_rx,
        recv.clone(),
        send.clone(),
    ));
    let file_join = tk::spawn(run_acceptor_file(
        cancel.clone(),
        file.0,
        file.1,
        file_out_rx,
        recv,
        send.clone(),
    ));
    (
        RemoteLinkHandle {
            remote_mcp_nom,
            cancel,
            msg_out_tx,
            file_out_tx,
            send,
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
}

impl RecvContext {
    fn new(
        self_mcp_nom: &str,
        peer_nom: &str,
    ) -> Self {
        let inbox_root = inbox_dir(self_mcp_nom).unwrap_or_else(|e| {
            eprintln!("grammar: remote inbox root unresolved ({e}); files will not land");
            PathBuf::from("/nonexistent")
        });
        Self {
            slots: Arc::new(std::sync::Mutex::new(HashMap::new())),
            peer_nom: peer_nom.to_string(),
            inbox_root,
        }
    }
}

/// The sender half: aggregate write-outcome tracking per outbound send. A send is
/// Sent once the Deliver and every file dest have written OK across both
/// connections, Unsent on the first write error or a link teardown with the send
/// still in flight. No peer frame is involved - the transport write is the ack.
#[derive(Clone)]
struct SendTracker {
    sends: Arc<std::sync::Mutex<HashMap<String, PendingSend>>>,
    /// The peer's McpNom, stamped into the Sent/Unsent report event.
    peer_nom: String,
}

/// One outbound send's outstanding writes: the file dests and the Deliver not yet
/// written OK. Complete (-> Sent) when both are empty/false.
struct PendingSend {
    pending_dests: HashSet<String>,
    message_pending: bool,
}

impl PendingSend {
    fn complete(&self) -> bool {
        self.pending_dests.is_empty() && !self.message_pending
    }
}

impl SendTracker {
    fn new(peer_nom: &str) -> Self {
        Self {
            sends: Arc::new(std::sync::Mutex::new(HashMap::new())),
            peer_nom: peer_nom.to_string(),
        }
    }

    /// Register a send before its frames are queued, so no write outcome can
    /// arrive for an unknown id.
    fn register(
        &self,
        id: &str,
        dests: &[String],
    ) {
        let mut map = self.sends.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            id.to_string(),
            PendingSend {
                pending_dests: dests.iter().cloned().collect(),
                message_pending: true,
            },
        );
    }

    /// Drop a registered send WITHOUT a report - the synchronous enqueue-failure
    /// path, where the caller already surfaces the error to the agent.
    fn forget(
        &self,
        id: &str,
    ) {
        self.sends.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }

    /// One dest's last chunk wrote OK; fire Sent if it completes the send.
    fn dest_written(
        &self,
        id: &str,
        dest: &str,
    ) {
        let done = {
            let mut map = self.sends.lock().unwrap_or_else(|e| e.into_inner());
            let complete = match map.get_mut(id) {
                Some(send) => {
                    send.pending_dests.remove(dest);
                    send.complete()
                }
                None => false,
            };
            if complete {
                map.remove(id);
            }
            complete
        };
        if done {
            report_sent(&self.peer_nom, id);
        }
    }

    /// The Deliver wrote OK; fire Sent if it completes the send.
    fn message_written(
        &self,
        id: &str,
    ) {
        let done = {
            let mut map = self.sends.lock().unwrap_or_else(|e| e.into_inner());
            let complete = match map.get_mut(id) {
                Some(send) => {
                    send.message_pending = false;
                    send.complete()
                }
                None => false,
            };
            if complete {
                map.remove(id);
            }
            complete
        };
        if done {
            report_sent(&self.peer_nom, id);
        }
    }

    /// A write for this send failed; fire Unsent once (idempotent per id).
    fn failed(
        &self,
        id: &str,
        kind: &str,
        message: &str,
    ) {
        let present = self
            .sends
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
            .is_some();
        if present {
            report_unsent(&self.peer_nom, id, kind, message);
        }
    }

    /// Link teardown: fire Unsent for every still-pending send. Idempotent - the
    /// drain empties the map, so a second driver's flush finds nothing.
    fn flush_unsent(
        &self,
        kind: &str,
        message: &str,
    ) {
        let drained: Vec<String> = {
            let mut map = self.sends.lock().unwrap_or_else(|e| e.into_inner());
            map.drain().map(|(id, _)| id).collect()
        };
        for id in drained {
            report_unsent(&self.peer_nom, &id, kind, message);
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

/// If the Deliver is in and every manifest file has landed, relay once. Called by
/// whichever driver completed the condition; the map remove makes it fire exactly
/// once. Nothing goes back to the sender - its Sent already fired from its own
/// write, and a receiver-side relay failure is intentionally not surfaced remotely
/// (only logged locally).
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
    if let Err((kind, message)) = relay_to_channel(ctx, id, &info, has_files) {
        eprintln!("grammar: remote delivery `{id}` not relayed onto the Channel: {kind}: {message}");
    }
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

/// Push a Sent report {id, mcp_nom} onto the LOCAL (sender's own) Channel.
fn report_sent(
    peer_nom: &str,
    id: &str,
) {
    let span = nu::Span::unknown();
    let mut event = nu::Record::new();
    event.insert("id", nu::Value::string(id.to_string(), span));
    event.insert("mcp_nom", nu::Value::string(peer_nom.to_string(), span));
    push_report(MODEL_SENT, nu::Value::record(event, span));
}

/// Push an Unsent report {id, mcp_nom, error{kind, message}} onto the LOCAL
/// (sender's own) Channel.
fn report_unsent(
    peer_nom: &str,
    id: &str,
    kind: &str,
    message: &str,
) {
    let span = nu::Span::unknown();
    let mut err = nu::Record::new();
    err.insert("kind", nu::Value::string(kind.to_string(), span));
    err.insert("message", nu::Value::string(message.to_string(), span));
    let mut event = nu::Record::new();
    event.insert("id", nu::Value::string(id.to_string(), span));
    event.insert("mcp_nom", nu::Value::string(peer_nom.to_string(), span));
    event.insert("error", nu::Value::record(err, span));
    push_report(MODEL_UNSENT, nu::Value::record(event, span));
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

/// Handle one inbound message-connection frame (a Deliver to relay).
fn handle_msg_frame(
    recv: &RecvContext,
    frame: MsgFrame,
) {
    let MsgFrame::Deliver {
        id,
        model,
        event_nuon,
        files,
    } = frame;
    record_deliver(recv, &id, DeliverInfo { model, event_nuon }, files);
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
    let domain = rv::ServerName::try_from(PLACEHOLDER_SERVER_NAME).map_err(io_other)?;
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

/// Drive the initiator's MESSAGE connection: send Deliver frames (reporting
/// Sent/Unsent on the write), relay inbound ones.
async fn run_initiator_message(
    cancel: tku::CancellationToken,
    mut framed_read: InitFramedRead,
    mut framed_write: InitFramedWrite,
    mut msg_out_rx: tk::UnboundedReceiver<MsgFrame>,
    recv: RecvContext,
    send: SendTracker,
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
                    let MsgFrame::Deliver { id, .. } = &frame;
                    let id = id.clone();
                    match framed_write.send(InitiatorToAcceptor::Msg(frame)).await {
                        Ok(()) => send.message_written(&id),
                        Err(_) => {
                            send.failed(&id, "write", "message connection write failed");
                            cancel.cancel();
                            break;
                        }
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(AcceptorToInitiator::Msg(frame))) => handle_msg_frame(&recv, frame),
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
    send.flush_unsent("link_down", "the remote link closed before the send completed");
}

/// Drive the initiator's FILE connection: FileChunk in (land) and out (send).
async fn run_initiator_file(
    cancel: tku::CancellationToken,
    mut framed_read: InitFramedRead,
    mut framed_write: InitFramedWrite,
    mut file_out_rx: tk::UnboundedReceiver<FileFrame>,
    recv: RecvContext,
    send: SendTracker,
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
                    let FileFrame::Chunk { id, dest, last, .. } = &frame;
                    let (id, dest, last) = (id.clone(), dest.clone(), *last);
                    match framed_write.send(InitiatorToAcceptor::File(frame)).await {
                        Ok(()) => {
                            if last {
                                send.dest_written(&id, &dest);
                            }
                        }
                        Err(_) => {
                            send.failed(&id, "write", "file connection write failed");
                            cancel.cancel();
                            break;
                        }
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
    send.flush_unsent("link_down", "the remote link closed before the send completed");
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

/// Drive the acceptor's MESSAGE connection: send Deliver frames (reporting
/// Sent/Unsent on the write), relay inbound ones.
async fn run_acceptor_message(
    cancel: tku::CancellationToken,
    mut framed_read: AcceptFramedRead,
    mut framed_write: AcceptFramedWrite,
    mut msg_out_rx: tk::UnboundedReceiver<MsgFrame>,
    recv: RecvContext,
    send: SendTracker,
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
                    let MsgFrame::Deliver { id, .. } = &frame;
                    let id = id.clone();
                    match framed_write.send(AcceptorToInitiator::Msg(frame)).await {
                        Ok(()) => send.message_written(&id),
                        Err(_) => {
                            send.failed(&id, "write", "message connection write failed");
                            cancel.cancel();
                            break;
                        }
                    }
                }
                None => break,
            },
            incoming = framed_read.next() => match incoming {
                Some(Ok(InitiatorToAcceptor::Msg(frame))) => handle_msg_frame(&recv, frame),
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
    send.flush_unsent("link_down", "the remote link closed before the send completed");
}

/// Drive the acceptor's FILE connection: FileChunk in (land) and out (send).
async fn run_acceptor_file(
    cancel: tku::CancellationToken,
    mut framed_read: AcceptFramedRead,
    mut framed_write: AcceptFramedWrite,
    mut file_out_rx: tk::UnboundedReceiver<FileFrame>,
    recv: RecvContext,
    send: SendTracker,
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
                    let FileFrame::Chunk { id, dest, last, .. } = &frame;
                    let (id, dest, last) = (id.clone(), dest.clone(), *last);
                    match framed_write.send(AcceptorToInitiator::File(frame)).await {
                        Ok(()) => {
                            if last {
                                send.dest_written(&id, &dest);
                            }
                        }
                        Err(_) => {
                            send.failed(&id, "write", "file connection write failed");
                            cancel.cancel();
                            break;
                        }
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
    send.flush_unsent("link_down", "the remote link closed before the send completed");
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

// ---- TLS config (known-public-key mTLS) ----

/// Build the client config: present our public key, match the peer's known one.
fn client_config(opts: &RemoteLinkOptions) -> io::Result<Arc<tls::ClientConfig>> {
    let verifier = Arc::new(PublicKeyVerifier::new(load_public_key(&opts.peer_public_key_file)?));
    let provider = Arc::new(rv::default_provider());
    let config = tls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io_other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(
            load_chain(&opts.self_public_key_file)?,
            load_key(&opts.self_private_key_file)?,
        )
        .map_err(io_other)?;
    Ok(Arc::new(config))
}

/// Build the server config: present our public key, match the client's known one.
fn server_config(
    self_public_key_file: &std::path::Path,
    self_private_key_file: &std::path::Path,
    peer_public_key_file: &std::path::Path,
) -> io::Result<Arc<tls::ServerConfig>> {
    let verifier = Arc::new(PublicKeyVerifier::new(load_public_key(peer_public_key_file)?));
    let provider = Arc::new(rv::default_provider());
    let config = tls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io_other)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            load_chain(self_public_key_file)?,
            load_key(self_private_key_file)?,
        )
        .map_err(io_other)?;
    Ok(Arc::new(config))
}

fn load_public_key(path: &std::path::Path) -> io::Result<rv::CertificateDer<'static>> {
    rv::CertificateDer::from_pem_file(path)
        .map_err(|e| io_other(format!("load public key {}: {e}", path.display())))
}

fn load_chain(path: &std::path::Path) -> io::Result<Vec<rv::CertificateDer<'static>>> {
    Ok(vec![load_public_key(path)?])
}

fn load_key(path: &std::path::Path) -> io::Result<tls::PrivateKeyDer<'static>> {
    tls::PrivateKeyDer::from_pem_file(path)
        .map_err(|e| io_other(format!("load key {}: {e}", path.display())))
}

fn io_other<E: Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}
