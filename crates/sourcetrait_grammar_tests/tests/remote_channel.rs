//! RemoteChannel runtime proof: two real grammar_mcp hosts linked over mTLS, each
//! with a simulated rmcp consumer (stdio JSON-RPC + a REAL WSS Channel client),
//! proving the transport end to end - the channel connect/verify/receive path, the
//! A->B->A application echo, and the mcp/remote/{Connected, Sent, Disconnected}
//! notice shapes. Certs are one-off and trusted directly, so this needs no OS
//! trust-store install and runs unattended. NO colony (dead pending redesign).

use std::time::Duration;

use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// The reusable consumer, standalone: connect a real WSS client to a host's
/// Channel against a test-minted CA, verify, and receive a body-emitted packet.
#[tested]
fn consumer_connects_verifies_and_receives_a_packet() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_certs("grammar", &root.join("channel"));
    let config = write_host_config(&root.join("host"), &ch.certs_dir);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "chan", "--namespace", "default", "--config", config.to_str().unwrap()],
        &root.join("host"),
    );
    let mut c = Consumer::attach(host, &ch.authority_public);

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

/// The full transport proof: A -> B -> A over a real mTLS link.
#[tested]
fn remote_loopback_echo() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    // Shared one-off channel cert (both hosts present entity_grammar; both
    // consumers trust the same CA). Cross-known remote-link leaves for A and B.
    let ch = generate_certs("grammar", &root.join("channel"));
    let cert_a = generate_certs("a", &root.join("cert_a"));
    let cert_b = generate_certs("b", &root.join("cert_b"));

    let listen = format!("127.0.0.1:{}", free_port());

    // Host B: the listener (binds, waits).
    let b_dir = root.join("host_b");
    let config_b = write_host_config(&b_dir, &ch.certs_dir);
    write_remotes_listener(&b_dir, &listen, &cert_b, &cert_a.entity_public);
    let host_b = Host::spawn_in(
        &root.join("scratch_b"),
        &["--id", "loop_b", "--namespace", "default", "--config", config_b.to_str().unwrap()],
        &b_dir,
    );
    let mut b = Consumer::attach(host_b, &ch.authority_public);

    // Host A: the connector (dials B).
    let a_dir = root.join("host_a");
    let config_a = write_host_config(&a_dir, &ch.certs_dir);
    write_remotes_connector(&a_dir, &listen, &cert_a, &cert_b.entity_public);
    let host_a = Host::spawn_in(
        &root.join("scratch_a"),
        &["--id", "loop_a", "--namespace", "default", "--config", config_a.to_str().unwrap()],
        &a_dir,
    );
    let mut a = Consumer::attach(host_a, &ch.authority_public);

    let nom_a = mcp_nom(&mut a);
    let nom_b = mcp_nom(&mut b);

    // Open the link: listener first, then the connector (retried until the bind
    // is up - the only non-obvious sync point, since there is no "listening"
    // notification to wait on).
    let _ = b.host.call("remote_channel_open", json!({"alias": "peer"}));
    let conn_a = open_connector_until_connected(&mut a);
    let conn_b = b
        .await_packet(|p| p.model == "mcp/remote/Connected", Duration::from_secs(15))
        .expect("B should see Connected");
    assert!(conn_a.event.contains(&nom_b), "A's Connected names B; got {conn_a:?}");
    assert!(conn_b.event.contains(&nom_a), "B's Connected names A; got {conn_b:?}");

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
}

/// A connector whose dial cannot land emits mcp/remote/Disconnected {alias, error}.
#[tested]
fn remote_open_failure_emits_disconnected() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir();

    let ch = generate_certs("grammar", &root.join("channel"));
    let cert_a = generate_certs("a", &root.join("cert_a"));
    let cert_dead = generate_certs("dead", &root.join("cert_dead"));

    let dead = format!("127.0.0.1:{}", free_port());
    let host_dir = root.join("host");
    let config = write_host_config(&host_dir, &ch.certs_dir);
    write_remotes_connector(&host_dir, &dead, &cert_a, &cert_dead.entity_public);
    let host = Host::spawn_in(
        &root.join("scratch"),
        &["--id", "dc", "--namespace", "default", "--config", config.to_str().unwrap()],
        &host_dir,
    );
    let mut c = Consumer::attach(host, &ch.authority_public);

    let open = c.host.call("remote_channel_open", json!({"alias": "peer"}));
    assert!(!has_error_path(&open), "open is void on a partial success; got {open}");

    let dc = c
        .await_packet(|p| p.model == "mcp/remote/Disconnected", Duration::from_secs(15))
        .expect("a failed dial should emit mcp/remote/Disconnected");
    assert!(dc.event.contains("peer"), "Disconnected on a failed open names the alias; got {dc:?}");
    assert!(dc.event.contains("error"), "Disconnected on a failed open carries an error; got {dc:?}");
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

/// Open a connector link, retrying until Connected - a Disconnected (the dial
/// beat the listener's bind) or a timeout just retries. The listener registers
/// nothing on a failed connect, so a re-open is always allowed.
fn open_connector_until_connected(c: &mut Consumer) -> Packet {
    for _ in 0..25 {
        let _ = c.host.call("remote_channel_open", json!({"alias": "peer"}));
        if let Some(p) = c.await_packet(
            |p| p.model == "mcp/remote/Connected" || p.model == "mcp/remote/Disconnected",
            Duration::from_secs(2),
        ) && p.model == "mcp/remote/Connected"
        {
            return p;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("connector never reached Connected");
}
