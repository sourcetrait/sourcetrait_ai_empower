#![allow(dead_code)]
//! Remote link lifecycle: two entity-pinned mTLS connections to one peer,
//! message/control + file, driven by a per-link handle.
use crate::*;

/// Server-name presented on the client handshake; entity-pin ignores it.
const PIN_SERVER_NAME: &str = "grammar.invalid";

/// How long a closing side waits for the reciprocal `Close` before giving up.
const CLOSE_FRAME_GRACE: tk::TkDuration = tk::TkDuration::from_secs(2);

/// What an initiator needs to open a link to a remote acceptor.
pub(crate) struct RemoteLinkOptions {
    /// The acceptor's listen address.
    pub addr: std::net::SocketAddr,
    /// This host's own entity leaf (srcert `entity_grammar.pem`), presented as
    /// the client certificate.
    pub self_cert_file: PathBuf,
    /// The private key paired with `self_cert_file`.
    pub self_key_file: PathBuf,
    /// The peer's entity leaf; the presented leaf must match it byte-for-byte.
    pub remote_pin_file: PathBuf,
}

/// A running link: the peer McpNom, a shared cancel, and both driver tasks.
pub(crate) struct RemoteLinkHandle {
    pub remote_mcp_nom: String,
    cancel: tku::CancellationToken,
    message_join: Option<tk::JoinHandle<()>>,
    file_join: Option<tk::JoinHandle<()>>,
}

impl RemoteLinkHandle {
    /// Cancel both drivers, wait up to `timeout` for their close handshakes,
    /// then abort any that overran.
    pub(crate) async fn close(&mut self, timeout: tk::TkDuration) {
        if !self.cancel.is_cancelled() {
            self.cancel.cancel();
        }
        for join in [self.message_join.take(), self.file_join.take()] {
            let Some(handle) = join else { continue };
            if handle.is_finished() {
                continue;
            }
            let abort = handle.abort_handle();
            if tk::timeout(timeout, handle).await.is_err() {
                abort.abort();
            }
        }
    }
}

/// Namespace for the link constructors.
pub(crate) struct RemoteLink;

impl RemoteLink {
    /// Open a link as the initiator: two mTLS connections (message + file),
    /// each handshaked, both drivers spawned under one cancel.
    pub(crate) async fn connect(
        self_mcp_nom: &str,
        opts: RemoteLinkOptions,
    ) -> io::Result<RemoteLinkHandle> {
        let connector = tls::TlsConnector::from(client_config(&opts)?);
        let (remote_msg, msg_read, msg_write) =
            connect_conn(&connector, opts.addr, self_mcp_nom, RemoteStream::Message).await?;
        let (remote_file, file_read, file_write) =
            connect_conn(&connector, opts.addr, self_mcp_nom, RemoteStream::File).await?;
        pair_check(&remote_msg, &remote_file)?;
        let cancel = tku::CancellationToken::new();
        let message_join = tk::spawn(run_initiator_conn(cancel.clone(), msg_read, msg_write));
        let file_join = tk::spawn(run_initiator_conn(cancel.clone(), file_read, file_write));
        Ok(RemoteLinkHandle {
            remote_mcp_nom: remote_msg,
            cancel,
            message_join: Some(message_join),
            file_join: Some(file_join),
        })
    }

    /// Accept ONE link's two connections off `listener` and start its drivers.
    /// Slice-D scope: a single link; the multi-link listener + registry is leg 4.
    pub(crate) async fn accept(
        self_mcp_nom: &str,
        listener: &tk::TcpListener,
        acceptor: &tls::TlsAcceptor,
    ) -> io::Result<RemoteLinkHandle> {
        let mut message: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
        let mut file: Option<(String, AcceptFramedRead, AcceptFramedWrite)> = None;
        while message.is_none() || file.is_none() {
            let (tcp, _peer) = listener.accept().await?;
            let (remote, stream, framed_read, framed_write) =
                accept_conn(acceptor, self_mcp_nom, tcp).await?;
            match stream {
                RemoteStream::Message => message = Some((remote, framed_read, framed_write)),
                RemoteStream::File => file = Some((remote, framed_read, framed_write)),
            }
        }
        let (remote_msg, msg_read, msg_write) = message.expect("message half present");
        let (remote_file, file_read, file_write) = file.expect("file half present");
        pair_check(&remote_msg, &remote_file)?;
        let cancel = tku::CancellationToken::new();
        let message_join = tk::spawn(run_acceptor_conn(cancel.clone(), msg_read, msg_write));
        let file_join = tk::spawn(run_acceptor_conn(cancel.clone(), file_read, file_write));
        Ok(RemoteLinkHandle {
            remote_mcp_nom: remote_msg,
            cancel,
            message_join: Some(message_join),
            file_join: Some(file_join),
        })
    }
}

/// Both connections of one link must report the same peer McpNom.
fn pair_check(message: &str, file: &str) -> io::Result<()> {
    if message == file {
        Ok(())
    } else {
        Err(io_other(format!(
            "link peers disagree: message conn `{message}`, file conn `{file}`"
        )))
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

/// Drive the initiator side until close or cancel (slice D: Close only).
async fn run_initiator_conn(
    cancel: tku::CancellationToken,
    mut framed_read: InitFramedRead,
    mut framed_write: InitFramedWrite,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_initiator(&mut framed_read, &mut framed_write).await;
                break;
            }
            msg = framed_read.next() => match msg {
                Some(Ok(AcceptorToInitiator::Close)) => {
                    let _ = framed_write.send(InitiatorToAcceptor::Close).await;
                    break;
                }
                Some(Ok(AcceptorToInitiator::Hello { .. })) => {}
                Some(Err(_)) => break,
                None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Send `Close` and wait briefly for the peer's reciprocal `Close`.
async fn close_initiator(framed_read: &mut InitFramedRead, framed_write: &mut InitFramedWrite) {
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

/// Drive the acceptor side until close or cancel (slice D: Close only).
async fn run_acceptor_conn(
    cancel: tku::CancellationToken,
    mut framed_read: AcceptFramedRead,
    mut framed_write: AcceptFramedWrite,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                close_acceptor(&mut framed_read, &mut framed_write).await;
                break;
            }
            msg = framed_read.next() => match msg {
                Some(Ok(InitiatorToAcceptor::Close)) => {
                    let _ = framed_write.send(AcceptorToInitiator::Close).await;
                    break;
                }
                Some(Ok(InitiatorToAcceptor::Hello { .. })) => {}
                Some(Err(_)) => break,
                None => break,
            },
        }
    }
    let _ = framed_write.close().await;
}

/// Send `Close` and wait briefly for the peer's reciprocal `Close`.
async fn close_acceptor(framed_read: &mut AcceptFramedRead, framed_write: &mut AcceptFramedWrite) {
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
    let verifier = Arc::new(EntityPin::new(load_leaf(&opts.remote_pin_file)?));
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
    remote_pin_file: &std::path::Path,
) -> io::Result<Arc<tls::ServerConfig>> {
    let verifier = Arc::new(EntityPin::new(load_leaf(remote_pin_file)?));
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
