// P3 probe (channel campaign, plan_channels REV 11): does the agent-side Monitor's
// `ws` source accept `wss://localhost:<port>` presenting the leaf our own CA issued,
// with that CA installed in the system trust store?
//
// Every other wire question is already answered - P2 reachability, P7 rate, P8 the
// 1 MiB ceiling, P9 fidelity, P11 claim - so this does the SMALLEST thing that can
// answer the TLS one: one connection, two packets, a clean close. The RFC 6455 half
// is carried over unchanged from the plaintext probe, so a failure here is
// attributable to TLS and to nothing else.
//
// It also closes P11's one gap. That probe never observed a PLANNED close (its
// process hit the accept deadline first), so a dead host and a clean shutdown both
// showed as a bare 1006. This one sends an explicit 1000 + reason at the end, to see
// whether the client distinguishes them.
//
// It lives as an example in THIS crate so it links the workspace's already-compiled
// rustls (0.23 on ring) instead of building a second TLS stack. rustls defaults to
// aws_lc_rs, so the dep is pinned default-features = false + ring; the provider is
// passed explicitly here rather than read from the process-global, so nothing about
// the result depends on ambient state.
//
// argv: <port> <cert-dir> <log-path> <accept-seconds> <hold-seconds>

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let port: u16 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(47810);
    let cert_dir = a.get(2).cloned().unwrap_or_else(|| ".".into());
    let log = a.get(3).cloned().unwrap_or_else(|| "p3.log".into());
    let accept_secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(180);
    let hold_secs: u64 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(30);

    let leaf = std::path::Path::new(&cert_dir).join("entity_grammar.pem");
    let key = std::path::Path::new(&cert_dir).join("entity_grammar.key.pem");

    let certs = match CertificateDer::from_pem_file(&leaf) {
        Ok(c) => vec![c],
        Err(e) => return append(&log, &format!("=== FATAL leaf {}: {e} ===\n", leaf.display())),
    };
    let key = match PrivateKeyDer::from_pem_file(&key) {
        Ok(k) => k,
        Err(e) => return append(&log, &format!("=== FATAL key {e} ===\n")),
    };
    append(&log, &format!("=== loaded leaf ({} cert) + key ===\n", certs.len()));

    // Explicit provider: the ambient process-global is not installed in this binary,
    // and passing it by hand keeps the probe's result independent of that.
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = match ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|b| b.with_no_client_auth().with_single_cert(certs, key))
    {
        Ok(c) => Arc::new(c),
        Err(e) => return append(&log, &format!("=== FATAL server config: {e} ===\n")),
    };

    // Bind BOTH loopback families. `localhost` may resolve to either, and a probe
    // that answers only one turns "the client refused our certificate" into
    // "connection refused" - two very different findings that must not be confused.
    let mut threads = Vec::new();
    for addr in [format!("127.0.0.1:{port}"), format!("[::1]:{port}")] {
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                append(&log, &format!("=== bind FAILED {addr}: {e} ===\n"));
                continue;
            }
        };
        let _ = listener.set_nonblocking(true);
        append(&log, &format!("=== listening {addr} (tls) ===\n"));
        let (log, config) = (log.clone(), config.clone());
        threads.push(std::thread::spawn(move || {
            accept_loop(listener, &addr, &log, config, accept_secs, hold_secs)
        }));
    }
    if threads.is_empty() {
        return append(&log, "=== FATAL no listener bound ===\n");
    }
    for t in threads {
        let _ = t.join();
    }
    append(&log, "=== done ===\n");
}

fn accept_loop(
    listener: TcpListener,
    addr: &str,
    log: &str,
    config: Arc<ServerConfig>,
    accept_secs: u64,
    hold_secs: u64,
) {
    let deadline = Instant::now() + Duration::from_secs(accept_secs);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, peer)) => {
                append(log, &format!("{} conn {peer} on {addr}\n", stamp()));
                serve(stream, log, config.clone(), hold_secs);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => append(log, &format!("{} accept err: {e}\n", stamp())),
        }
    }
}

fn serve(mut tcp: TcpStream, log: &str, config: Arc<ServerConfig>, hold_secs: u64) {
    let _ = tcp.set_nonblocking(false);
    let _ = tcp.set_read_timeout(Some(Duration::from_millis(5000)));

    let mut conn = match ServerConnection::new(config) {
        Ok(c) => c,
        Err(e) => return append(log, &format!("{} server conn: {e}\n", stamp())),
    };
    // Drive the handshake explicitly rather than letting StreamOwned do it lazily:
    // a client that rejects our certificate sends a TLS ALERT, and this is where it
    // surfaces with a name. That server-side view is half of P3's answer - the other
    // half is whatever the Monitor reports - and a lazy handshake would bury it
    // inside the first websocket read.
    match conn.complete_io(&mut tcp) {
        Ok((rd, wr)) => append(
            log,
            &format!(
                "{} TLS OK read={rd} wrote={wr} version={:?} suite={:?} sni={:?}\n",
                stamp(),
                conn.protocol_version(),
                conn.negotiated_cipher_suite().map(|s| s.suite()),
                conn.server_name(),
            ),
        ),
        Err(e) => {
            append(log, &format!("{} TLS HANDSHAKE FAILED: {e}\n", stamp()));
            return;
        }
    }

    let version = format!("{:?}", conn.protocol_version());
    let suite = format!("{:?}", conn.negotiated_cipher_suite().map(|s| s.suite()));
    let mut tls = StreamOwned::new(conn, tcp);

    if !handshake(&mut tls, log) {
        return;
    }
    // The plan's step-3 verification shape: the agent VERIFIES by SEEING a real
    // packet, so this is the packet it would see.
    let open = r#"{nom: "p3a", from: "host", kind: "ChannelOpen", data: {probe: "p3", tls: true}}"#;
    send(&mut tls, log, open);
    // Second packet carries what TLS actually negotiated, so the agent-side record
    // proves the encrypted path end to end rather than merely that bytes arrived.
    let info = format!(
        r#"{{nom: "p3b", from: "host", kind: "TlsInfo", data: {{version: "{}", suite: "{}"}}}}"#,
        escape(&version),
        escape(&suite),
    );
    send(&mut tls, log, &info);

    append(log, &format!("{} holding {hold_secs}s\n", stamp()));
    std::thread::sleep(Duration::from_secs(hold_secs));

    // P11's gap: a PLANNED close, so the agent can tell shutdown from a crash.
    append(log, &format!("{} sending close 1000\n", stamp()));
    let _ = tls.write_all(&close_frame(1000, "p3 probe complete"));
    let _ = tls.flush();
    std::thread::sleep(Duration::from_millis(250));
}

fn send(s: &mut impl Write, log: &str, payload: &str) {
    match s.write_all(&text_frame(payload)).and_then(|()| s.flush()) {
        Ok(()) => append(log, &format!("{} sent {} bytes\n", stamp(), payload.len())),
        Err(e) => append(log, &format!("{} send failed: {e}\n", stamp())),
    }
}

/// Complete the RFC 6455 upgrade over an already-established TLS stream. Unchanged
/// from the plaintext probe apart from being generic over the stream.
fn handshake(s: &mut (impl Read + Write), log: &str) -> bool {
    let mut buf = [0u8; 8192];
    let n = s.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]).to_string();

    let Some(key) = req
        .lines()
        .find_map(|l| l.strip_prefix("Sec-WebSocket-Key:"))
        .map(|v| v.trim().to_string())
    else {
        append(log, &format!("{} no Sec-WebSocket-Key; refusing\n", stamp()));
        let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        return false;
    };

    let accept = b64(&sha1(format!("{key}{GUID}").as_bytes()));
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if s.write_all(resp.as_bytes()).is_err() {
        append(log, &format!("{} handshake write failed\n", stamp()));
        return false;
    }
    let _ = s.flush();
    append(log, &format!("{} ws handshake ok (key {key})\n", stamp()));
    true
}

/// Newline-escape for the wire. P9 found frames BATCH into one event joined by
/// newlines, so a literal newline inside a frame is indistinguishable from a batch
/// boundary - the same invariant `grimm dbg` already enforces on debug.nuonl.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('"', "\\\"")
}

fn stamp() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("[{ms}]")
}

fn text_frame(payload: &str) -> Vec<u8> {
    frame(0x81, payload.as_bytes())
}

fn close_frame(code: u16, reason: &str) -> Vec<u8> {
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(reason.as_bytes());
    frame(0x88, &payload)
}

fn frame(first_byte: u8, b: &[u8]) -> Vec<u8> {
    let mut f = vec![first_byte];
    if b.len() < 126 {
        f.push(b.len() as u8);
    } else if b.len() <= 65535 {
        f.push(126);
        f.extend_from_slice(&(b.len() as u16).to_be_bytes());
    } else {
        f.push(127);
        f.extend_from_slice(&(b.len() as u64).to_be_bytes());
    }
    f.extend_from_slice(b);
    f // server frames are never masked
}

fn append(path: &str, s: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(s.as_bytes());
    }
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if c.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let ml = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}
