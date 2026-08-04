//! RemoteChannel test harness: a reusable simulated rmcp consumer - the existing
//! stdio `Host` plus a REAL WSS Channel client that trusts a one-off test CA - and
//! the one-off key + config plumbing two linked hosts need.
//!
//! No test seam: the consumer connects to a host's Channel exactly as
//! claude-code's Monitor does (a real TLS + WebSocket client), so it settles the
//! observation question and is reusable to script any packet flow. Keys are
//! one-off and trusted directly via a private root store - never the OS trust
//! store, so the whole thing runs unattended with no OS-level install / sudo.
//!
//! Wire packets are NUON, so they are parsed by driving the consumer's own host
//! (`from nuon`) rather than pulling the nushell crates into this crate - the host
//! is the format authority, and it keeps the tests crate free of the nushell
//! eval stack (whose feature unification does not survive here).

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::json;
use sourcetrait_cert_lib as lib_cert;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tokio_rustls::rustls::pki_types::pem::PemObject;

use crate::{Host, has_error_path, structured};

// ---- one-off keys (trusted directly; never the OS trust store) ----

/// The key material a `generate` run leaves under `<dir>/certs`.
pub struct GeneratedKeys {
    /// `<dir>/certs` - the directory a host's `[channel].cert_dir` points at.
    pub dir: PathBuf,
    /// The self-signed CA public key a WSS client trusts as its sole root.
    pub ca: PathBuf,
    /// This host's own public key (the channel key, or a remote peer's known key).
    pub public_key: PathBuf,
    /// The matching private key.
    pub private_key: PathBuf,
}

fn key_config(name: &str) -> lib_cert::config::CertGenConfig {
    lib_cert::config::CertGenConfig {
        name: name.to_string(),
        organization: "SourceTrait".to_string(),
        validity_days: 3650,
        authority: lib_cert::AuthorityConfig {
            common_name: format!("Test {name} CA"),
        },
        entity: lib_cert::EntityConfig {
            common_name: "localhost".to_string(),
            subject_alt_names: vec![
                "127.0.0.1".to_string(),
                "::1".to_string(),
                "localhost".to_string(),
            ],
            usages: vec![lib_cert::EntityUsage::Server, lib_cert::EntityUsage::Client],
        },
    }
}

/// Mint a fresh self-signed CA + public/private key pair named `name` into
/// `<dir>/certs`. The channel key MUST use `name = "grammar"` (the hub loads the
/// public key named for it) with a 127.0.0.1 SAN; a remote link's keys take any
/// distinct name and are cross-known by byte match.
pub fn generate_keys(name: &str, dir: &Path) -> GeneratedKeys {
    let files = lib_cert::generate(&key_config(name), dir).expect("generate keys");
    GeneratedKeys {
        dir: lib_cert::certs_dir(dir),
        ca: files.authority_public,
        public_key: files.entity_public,
        private_key: files.entity_private,
    }
}

// ---- host config + remotes.toml plumbing ----

/// Write a `grammar_mcp.toml` overriding `[channel].cert_dir` to the test's own
/// channel keys, into `host_dir` (created). Returns the path for `--config`.
pub fn write_host_config(host_dir: &Path, channel_key_dir: &Path) -> PathBuf {
    std::fs::create_dir_all(host_dir).expect("mkdir host dir");
    let toml = format!("[channel]\ncert_dir = \"{}\"\n", channel_key_dir.display());
    let path = host_dir.join("grammar_mcp.toml");
    std::fs::write(&path, toml).expect("write grammar_mcp.toml");
    path
}

fn write_remotes(cwd: &Path, body: &str) {
    let dir = cwd.join(".grammar").join("mcp");
    std::fs::create_dir_all(&dir).expect("mkdir .grammar/mcp");
    std::fs::write(dir.join("remotes.toml"), body).expect("write remotes.toml");
}

/// A connector `[[remote]]` (dials `peer_addr`) at `<cwd>/.grammar/mcp/remotes.toml`.
pub fn write_remotes_connector(
    cwd: &Path,
    peer_addr: &str,
    self_keys: &GeneratedKeys,
    peer_public: &Path,
) {
    let body = format!(
        "[[remote]]\nalias = \"peer\"\naddress = \"{addr}\"\n\
         self_public_key_file = \"{sp}\"\nself_private_key_file = \"{sk}\"\n\
         public_key_file = \"{pk}\"\n",
        addr = peer_addr,
        sp = self_keys.public_key.display(),
        sk = self_keys.private_key.display(),
        pk = peer_public.display(),
    );
    write_remotes(cwd, &body);
}

/// A listener `[[remote]]` (binds `listen`) at `<cwd>/.grammar/mcp/remotes.toml`.
pub fn write_remotes_listener(
    cwd: &Path,
    listen: &str,
    self_keys: &GeneratedKeys,
    peer_public: &Path,
) {
    let body = format!(
        "[[remote]]\nalias = \"peer\"\nlisten = \"{listen}\"\n\
         self_public_key_file = \"{sp}\"\nself_private_key_file = \"{sk}\"\n\
         public_key_file = \"{pk}\"\n",
        listen = listen,
        sp = self_keys.public_key.display(),
        sk = self_keys.private_key.display(),
        pk = peer_public.display(),
    );
    write_remotes(cwd, &body);
}

/// A free loopback TCP port (bind :0, read it back, release). The tiny race is
/// made negligible by the `--test-threads=1` serial run.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind probe port");
    listener.local_addr().expect("probe local_addr").port()
}

// ---- the packet ----

/// One Channel packet: the scalar envelope fields, plus `event` re-rendered to
/// JSON for coarse `contains` assertions.
#[derive(Debug, Clone)]
pub struct Packet {
    pub id: String,
    pub from: String,
    pub model: String,
    pub attached: Option<String>,
    pub event: String,
}

/// Parse one NUON wire line by driving the host's own `from nuon` - the host is
/// the format authority. Returns None when the line is not a packet.
fn parse_line(host: &mut Host, line: &str) -> Option<Packet> {
    let resp = host.run(json!({
        "args_schema": {"line": "string"},
        "result_schema": {
            "id": "string", "from": "string", "model": "string",
            "attached": "string", "event": "string",
        },
        "args": {"line": line},
        "body": r#"let rec = (try { $args.line | from nuon } catch { {} })
{ id: ($rec.id? | default ""), from: ($rec.from? | default ""), model: ($rec.model? | default ""), attached: ($rec.attached? | default ""), event: ($rec.event? | default {} | to json) }"#,
    }));
    let result = structured(&resp).get("result")?;
    let field = |k: &str| result.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let model = field("model");
    if model.is_empty() {
        return None;
    }
    let attached = field("attached");
    Some(Packet {
        id: field("id"),
        from: field("from"),
        model,
        attached: (!attached.is_empty()).then_some(attached),
        event: field("event"),
    })
}

// ---- the WSS reader ----

fn wss_host_port(wss: &str) -> (String, u16) {
    let rest = wss.strip_prefix("wss://").unwrap_or(wss);
    let (host, port) = rest.rsplit_once(':').expect("wss host:port");
    (host.to_string(), port.parse().expect("wss port"))
}

fn client_config(ca_pem: &Path) -> Arc<tokio_rustls::rustls::ClientConfig> {
    let ca = CertificateDer::from_pem_file(ca_pem).expect("load test CA pem");
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    roots.add(ca).expect("add test CA to root store");
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let config = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

/// Connect a real TLS+WebSocket client to `wss`, trusting only `ca_pem`, and
/// forward every raw packet line to `tx`. The thread ends when the socket closes
/// (the host died) or the receiver is dropped.
fn spawn_ws_reader(wss: String, ca_pem: PathBuf, tx: Sender<String>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("ws reader runtime");
        rt.block_on(async move {
            let connector = tokio_rustls::TlsConnector::from(client_config(&ca_pem));
            let (host, port) = wss_host_port(&wss);
            let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
                .await
                .expect("wss tcp connect");
            let ip: IpAddr = host.parse().expect("wss host is an ip");
            let server_name = tokio_rustls::rustls::pki_types::ServerName::IpAddress(ip.into());
            let tls = connector.connect(server_name, tcp).await.expect("wss tls");
            // TLS is already established, so the ws handshake rides a `ws://` request.
            let request = format!("ws://{host}:{port}");
            let (mut ws, _resp) = tokio_tungstenite::client_async(request.as_str(), tls)
                .await
                .expect("wss handshake");
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => {
                        for line in t.as_str().lines() {
                            if !line.is_empty() && tx.send(line.to_string()).is_err() {
                                return;
                            }
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => return,
                    _ => {}
                }
            }
        });
    })
}

// ---- the simulated consumer ----

/// A simulated rmcp consumer: the stdio `Host` plus a live WSS Channel client.
pub struct Consumer {
    pub host: Host,
    rx: Receiver<String>,
    _reader: JoinHandle<()>,
}

impl Consumer {
    /// Open the host's Channel, connect a real WSS client trusting `ca_pem`, see
    /// the `mcp/channel/Open` packet, then verify - the claude-code handshake.
    pub fn attach(mut host: Host, ca_pem: &Path) -> Self {
        let open = host.call("channel_open", json!({}));
        let wss = structured(&open)["wss"]
            .as_str()
            .expect("channel_open returns a wss url")
            .to_string();
        let (tx, rx) = mpsc::channel();
        let reader = spawn_ws_reader(wss, ca_pem.to_path_buf(), tx);
        let first_raw = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the hub sends mcp/channel/Open on connect");
        let first = parse_line(&mut host, &first_raw).expect("the Open packet should parse");
        assert_eq!(
            first.model, "mcp/channel/Open",
            "the first packet must be the handshake; got {first:?}",
        );
        let verified = host.call("channel_verified", json!({}));
        assert!(
            !has_error_path(&verified),
            "channel_verified should succeed once claimed; got {verified}",
        );
        Self { host, rx, _reader: reader }
    }

    /// Block until a packet matching `pred` arrives, or `timeout` elapses. Each
    /// received line is parsed through the host before the predicate sees it.
    pub fn await_packet(
        &mut self,
        pred: impl Fn(&Packet) -> bool,
        timeout: Duration,
    ) -> Option<Packet> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let raw = self.rx.recv_timeout(remaining).ok()?;
            if let Some(packet) = parse_line(&mut self.host, &raw)
                && pred(&packet)
            {
                return Some(packet);
            }
        }
    }
}
