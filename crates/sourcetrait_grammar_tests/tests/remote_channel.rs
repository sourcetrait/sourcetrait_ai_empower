//! RemoteChannel runtime proof: two real grammar_mcp hosts linked over mTLS, each
//! with a simulated rmcp consumer (stdio JSON-RPC + a REAL WSS Channel client),
//! proving the transport end to end - the channel connect/verify/receive path, the
//! A->B->A application echo, the mcp/remote/{Connected, Disconnected} notice shapes,
//! and the RemoteFirstBlood fixes: the synchronous blocking open (ConnectionWoes),
//! the bound-listener `listening` set (RemoteChannelListeners), and the oversized
//! event auto-spill (CapNoCap). Keys are one-off and trusted directly, so this needs
//! no OS trust-store install and runs unattended. NO colony (dead pending redesign).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// The reusable consumer, standalone: connect a real WSS client to a host's
/// Channel against a test-minted CA, verify, and receive a body-emitted packet.
#[tested]
fn consumer_connects_verifies_and_receives_a_packet() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_keys("grammar", &root.join("channel"));
    let config = write_host_config(&root.join("host"), &ch.dir);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "chan", "--namespace", "default", "--config", config.to_str().unwrap()],
        &root.join("host"),
    );
    let mut c = Consumer::attach(host, &ch.ca);

    // A body-driven grimm channel_send must arrive on the real WSS client.
    let resp = c.host.run(json!({
        "args_schema": {},
        "result_schema": {"ok": "bool"},
        "args": {},
        "body": "grimm channel_send \"test/Ping\" {msg: \"hello-wss\"}; {ok: true}",
    }));
    assert!(!has_error_path(&resp), "channel_send body failed: {resp}");

    let packet = c
        .await_packet(|p| p.model == "test/Ping", Duration::from_secs(10))
        .expect("consumer should receive the test/Ping over WSS");
    assert!(
        packet.event.contains("hello-wss"),
        "the packet event should carry the payload; got {packet:?}",
    );
    assert_eq!(
        packet.from.split('/').next(),
        Some("thread"),
        "a body-origin packet is from `thread/<nonce>`; got {}",
        packet.from,
    );
}

/// The full transport proof plus the ConnectionWoes contract: a synchronous
/// blocking open (listener on bind, connector on connect), the A->B->A echo over
/// a real mTLS link, and Disconnected on teardown for both roles.
#[tested]
fn remote_loopback_echo() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();
    let (mut a, mut b, nom_a, nom_b) = spawn_two_hosts(&root);

    // The listener (B) opens - blocks on bind, returns synchronously once bound.
    // The connector (A) then opens - blocks on connect (B is already bound),
    // returns synchronously once paired. Neither open is void-then-async.
    pair(&mut a, &mut b, &nom_a);
    // The connector gets NO Connected for its open attempt (the synchronous open
    // success is the notice); a stray one within a grace window is a failure.
    assert!(
        a.await_packet(|p| p.model == "mcp/remote/Connected", Duration::from_secs(2)).is_none(),
        "the connector open attempt emits no Connected",
    );

    // A sends test/Ping to B; A sees its own mcp/remote/Sent (driven by the write).
    let send_id = remote_send(&mut a, &nom_b, "test/Ping", "note: \"ping-1\"");
    a.await_packet(
        |p| p.model == "mcp/remote/Sent" && p.event.contains(&send_id),
        Duration::from_secs(15),
    )
    .expect("A should see its own Sent for the ping");

    // B's agent observes the relayed packet on B's Channel...
    let relayed = b
        .await_packet(
            |p| p.model == "test/Ping" && p.from == format!("mcp/remote/{nom_a}"),
            Duration::from_secs(15),
        )
        .expect("B should receive the relayed test/Ping");
    assert!(relayed.event.contains("ping-1"), "relayed event carries the payload; got {relayed:?}");

    // ...and echoes it back to A as test/Loopback - the return leg.
    remote_send(&mut b, &nom_a, "test/Loopback", "echo: \"ping-1\"");
    let looped = a
        .await_packet(
            |p| p.model == "test/Loopback" && p.from == format!("mcp/remote/{nom_b}"),
            Duration::from_secs(15),
        )
        .expect("A should receive the test/Loopback echo - the round-trip");
    assert!(looped.event.contains("ping-1"), "the loopback carries the original payload; got {looped:?}");

    // Teardown: closing the link on A drops the established link; Disconnected is
    // retained for an established-link teardown on BOTH roles (the_user).
    let close = a.host.call("remote_channel_close", json!({"alias": "peer"}));
    assert!(!has_error_path(&close), "remote_channel_close succeeds on an open link; got {close}");
    a.await_packet(|p| p.model == "mcp/remote/Disconnected", Duration::from_secs(15))
        .expect("the connector sees Disconnected on teardown");
    b.await_packet(|p| p.model == "mcp/remote/Disconnected", Duration::from_secs(15))
        .expect("the listener sees Disconnected on teardown");
}

/// ConnectionWoes: a connector whose dial is refused fails the open SYNCHRONOUSLY
/// (remote::open_failed), with no async Disconnected for the open attempt. (The 20s
/// connect timeout on an accept-but-hang peer is covered by the code + const, not
/// separately forced here - it would cost a 20s wait per run.)
#[tested]
fn remote_connector_open_refused_is_synchronous() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_keys("grammar", &root.join("channel"));
    let cert_a = generate_keys("a", &root.join("cert_a"));
    let cert_dead = generate_keys("dead", &root.join("cert_dead"));

    // A free (unbound) port - a dial there is refused immediately (not a timeout).
    let dead = format!("127.0.0.1:{}", free_port());
    let host_dir = root.join("host");
    let config = write_host_config(&host_dir, &ch.dir);
    write_remotes_connector(&host_dir, &dead, &cert_a, &cert_dead.public_key);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "dc", "--namespace", "default", "--config", config.to_str().unwrap()],
        &host_dir,
    );
    let mut c = Consumer::attach(host, &ch.ca);

    let open = c.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(has_error_path(&open), "a refused connect fails the open synchronously; got {open}");
    assert_eq!(err_kind(&open), "remote::open_failed", "the sync failure kind; got {open}");
    assert!(
        c.await_packet(|p| p.model == "mcp/remote/Disconnected", Duration::from_secs(3)).is_none(),
        "the connector open attempt emits no async Disconnected",
    );
}

/// ConnectionWoes (subsumed double-open): a redundant listener open fails
/// SYNCHRONOUSLY on the bind ("address already in use") rather than slipping past
/// into a confusing async Disconnected.
#[tested]
fn remote_listener_double_open_fails_synchronously() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_keys("grammar", &root.join("channel"));
    let cert_b = generate_keys("b", &root.join("cert_b"));
    let cert_peer = generate_keys("peer", &root.join("cert_peer"));

    let listen = format!("127.0.0.1:{}", free_port());
    let host_dir = root.join("host");
    let config = write_host_config(&host_dir, &ch.dir);
    write_remotes_listener(&host_dir, &listen, &cert_b, &cert_peer.public_key);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "dl", "--namespace", "default", "--config", config.to_str().unwrap()],
        &host_dir,
    );
    let mut c = Consumer::attach(host, &ch.ca);

    let first = c.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&first), "the first listener open binds synchronously; got {first}");
    let second = c.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(has_error_path(&second), "a redundant listener open fails synchronously; got {second}");
    assert_eq!(err_kind(&second), "remote::open_failed", "the sync bind-clash kind; got {second}");
}

/// RemoteChannelListeners: a bound-but-unpaired listener shows under `listening`
/// (invisible before), and moves into `channels` once a peer pairs.
#[tested]
fn remote_channels_lists_bound_listener() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();
    let (mut a, mut b, nom_a, _nom_b) = spawn_two_hosts(&root);

    // B's listener binds (synchronous) but has no peer yet.
    let open_b = b.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&open_b), "listener open is synchronous on bind; got {open_b}");
    let rc = remote_channels_result(&mut b);
    assert!(
        listing_names(&rc["listening"], "remote").contains(&"peer".to_string()),
        "a bound-waiting listener shows under `listening`; got {rc}",
    );
    assert!(
        listing_names(&rc["channels"], "alias").is_empty(),
        "nothing is established yet; got {rc}",
    );

    // Pair: A connects, B sees Connected, the listener moves to `channels`.
    let open_a = a.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&open_a), "connector open is synchronous on connect; got {open_a}");
    b.await_packet(
        |p| p.model == "mcp/remote/Connected" && p.event.contains(&nom_a),
        Duration::from_secs(15),
    )
    .expect("B sees Connected on pairing");

    let rc2 = remote_channels_result(&mut b);
    assert!(
        listing_names(&rc2["channels"], "alias").contains(&"peer".to_string()),
        "the paired listener is now an established channel; got {rc2}",
    );
    assert!(
        listing_names(&rc2["listening"], "remote").is_empty(),
        "the listener left `listening` once paired; got {rc2}",
    );
}

/// CapNoCap (local): an oversized channel_send event auto-spills to an inbox file;
/// a compact pointer rides the wire under the notification cap, and the full event
/// survives in the spill file.
#[tested]
fn channel_send_spills_oversized_event() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_keys("grammar", &root.join("channel"));
    let config = write_host_config(&root.join("host"), &ch.dir);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "spill", "--namespace", "default", "--config", config.to_str().unwrap()],
        &root.join("host"),
    );
    let mut c = Consumer::attach(host, &ch.ca);
    let nom = mcp_nom(&mut c);

    let resp = c.host.run(json!({
        "args_schema": {},
        "result_schema": {"ok": "bool"},
        "args": {},
        "body": "grimm channel_send \"test/Big\" {marker: \"SPILL_MARKER_42\", blob: (1..3000 | each { \"x\" } | str join)}; {ok: true}",
    }));
    assert!(!has_error_path(&resp), "channel_send body failed: {resp}");

    let pkt = c
        .await_packet(|p| p.model == "test/Big", Duration::from_secs(10))
        .expect("consumer should receive the test/Big packet");
    assert!(
        pkt.event.contains("spilled_event_path"),
        "an oversized event is replaced by a spill pointer; got {pkt:?}",
    );
    assert!(
        !pkt.event.contains("SPILL_MARKER_42"),
        "the event content is spilled out of the wire packet; got {pkt:?}",
    );
    assert!(
        pkt.raw.chars().count() < 500,
        "the pointer packet stays under the notification cap; got {} chars",
        pkt.raw.chars().count(),
    );

    let content = read_inbox(&nom, &spill_path(&pkt));
    assert!(
        content.contains("SPILL_MARKER_42"),
        "the full event survives in the spill file; got {content}",
    );
}

/// CapNoCap (remote): an oversized remote_channel_send event auto-spills via the
/// file connection; the relayed packet on the receiver is a pointer under the cap,
/// and the full event lands in the receiver's inbox before the packet is seen.
#[tested]
fn remote_send_spills_oversized_event() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();
    let (mut a, mut b, nom_a, nom_b) = spawn_two_hosts(&root);
    pair(&mut a, &mut b, &nom_a);

    let big = "marker: \"REMOTE_SPILL_99\", blob: (1..3000 | each { \"x\" } | str join)";
    remote_send(&mut a, &nom_b, "test/BigRemote", big);

    let relayed = b
        .await_packet(
            |p| p.model == "test/BigRemote" && p.from == format!("mcp/remote/{nom_a}"),
            Duration::from_secs(15),
        )
        .expect("B should receive the relayed oversized event");
    assert!(
        relayed.event.contains("spilled_event_path"),
        "the relayed oversized event is a spill pointer; got {relayed:?}",
    );
    assert!(
        !relayed.event.contains("REMOTE_SPILL_99"),
        "the event content is spilled out of the wire packet; got {relayed:?}",
    );
    assert!(
        relayed.raw.chars().count() < 500,
        "the relayed pointer stays under the notification cap; got {} chars",
        relayed.raw.chars().count(),
    );
    assert!(relayed.attached.is_some(), "the relayed packet names the delivery dir; got {relayed:?}");

    let content = read_inbox(&nom_b, &spill_path(&relayed));
    assert!(
        content.contains("REMOTE_SPILL_99"),
        "the full event survives in B's inbox spill file; got {content}",
    );
}

// ---- helpers ----

fn mcp_nom(c: &mut Consumer) -> String {
    structured(&c.host.call("info", json!({})))["mcp_nom"]
        .as_str()
        .expect("info carries mcp_nom")
        .to_string()
}

/// Drive `grimm remote_channel_send` from a body and return the message id.
fn remote_send(c: &mut Consumer, peer_nom: &str, model: &str, event_body: &str) -> String {
    let body = format!("let id = (grimm remote_channel_send $args.nom \"{model}\" {{{event_body}}}); {{id: $id}}");
    let resp = c.host.run(json!({
        "args_schema": {"nom": "string"},
        "result_schema": {"id": "string"},
        "args": {"nom": peer_nom},
        "body": body,
    }));
    assert!(!has_error_path(&resp), "remote_channel_send body failed: {resp}");
    structured(&resp)["result"]["id"].as_str().expect("send returns a message id").to_string()
}

/// The first error diagnostic's `kind` from an error envelope.
fn err_kind(resp: &Value) -> String {
    structured(resp)["error"]["errors"][0]["kind"].as_str().unwrap_or("").to_string()
}

/// remote_channels()'s structured result, owned.
fn remote_channels_result(c: &mut Consumer) -> Value {
    structured(&c.host.call("remote_channels", json!({}))).clone()
}

/// The values under `key` across a remote_channels listing array.
fn listing_names(arr: &Value, key: &str) -> Vec<String> {
    arr.as_array()
        .map(|a| a.iter().filter_map(|e| e[key].as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// The inbox-relative spill path from a spill-pointer packet's event.
fn spill_path(pkt: &Packet) -> String {
    let ev: Value = serde_json::from_str(&pkt.event).expect("event parses as json");
    ev["spilled_event_path"].as_str().expect("a spill pointer carries spilled_event_path").to_string()
}

/// Read a spilled event file from a host's inbox (`$XDGX_SHM_DIR/mcp/<nom>/inbox`).
fn read_inbox(nom: &str, rel: &str) -> String {
    let shm = std::env::var("XDGX_SHM_DIR").expect("XDGX_SHM_DIR is set");
    let path = Path::new(&shm).join("mcp").join(nom).join("inbox").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read spill file {}: {e}", path.display()))
}

/// Spawn a listener host (B) and a connector host (A) cross-configured on one free
/// loopback port, each with an attached consumer. Returns (a, b, nom_a, nom_b); no
/// link is opened yet.
fn spawn_two_hosts(root: &Path) -> (Consumer, Consumer, String, String) {
    let ch = generate_keys("grammar", &root.join("channel"));
    let cert_a = generate_keys("a", &root.join("cert_a"));
    let cert_b = generate_keys("b", &root.join("cert_b"));
    let listen = format!("127.0.0.1:{}", free_port());

    let b_dir = root.join("host_b");
    let config_b = write_host_config(&b_dir, &ch.dir);
    write_remotes_listener(&b_dir, &listen, &cert_b, &cert_a.public_key);
    let host_b = Host::spawn_in(
        &root.join("scratch_b"),
        &["--id", "link_b", "--namespace", "default", "--config", config_b.to_str().unwrap()],
        &b_dir,
    );
    let mut b = Consumer::attach(host_b, &ch.ca);

    let a_dir = root.join("host_a");
    let config_a = write_host_config(&a_dir, &ch.dir);
    write_remotes_connector(&a_dir, &listen, &cert_a, &cert_b.public_key);
    let host_a = Host::spawn_in(
        &root.join("scratch_a"),
        &["--id", "link_a", "--namespace", "default", "--config", config_a.to_str().unwrap()],
        &a_dir,
    );
    let mut a = Consumer::attach(host_a, &ch.ca);

    let nom_a = mcp_nom(&mut a);
    let nom_b = mcp_nom(&mut b);
    (a, b, nom_a, nom_b)
}

/// Open the listener (B) then the connector (A) - both synchronous - and wait for
/// B's Connected, so the link is established when this returns.
fn pair(a: &mut Consumer, b: &mut Consumer, nom_a: &str) {
    let open_b = b.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&open_b), "listener open is synchronous on bind; got {open_b}");
    let open_a = a.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&open_a), "connector open is synchronous on connect; got {open_a}");
    b.await_packet(
        |p| p.model == "mcp/remote/Connected" && p.event.contains(nom_a),
        Duration::from_secs(15),
    )
    .expect("B sees Connected on pairing");
}
