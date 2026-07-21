use crate::*;
use crate::server::channel::state::{MsgId, escape_line};

/// Install a hub-shaped channel without a hub: the handle only ever holds the two
/// senders, the shutdown oneshot and the claimed flag, so the state machine is testable
/// without binding a socket or reading a certificate.
fn open_handle() -> (
    ChannelHandle,
    tk::UnboundedReceiver<String>,
    tk::oneshot::Receiver<(u16, String)>,
    Arc<AtomicBool>,
) {
    let handle = ChannelHandle::new();
    let (packets, packet_rx) = tk::unbounded_channel::<String>();
    let (close, close_rx) = tk::oneshot::channel::<(u16, String)>();
    let (shutdown, _shutdown_rx) = tk::oneshot::channel::<()>();
    let claimed = Arc::new(AtomicBool::new(false));
    handle.install(
        "wss://127.0.0.1:1".to_string(),
        packets,
        close,
        shutdown,
        claimed.clone(),
    );
    (handle, packet_rx, close_rx, claimed)
}

/// The ordinary case: a peer has connected and claimed.
fn claimed_handle() -> (
    ChannelHandle,
    tk::UnboundedReceiver<String>,
    tk::oneshot::Receiver<(u16, String)>,
) {
    let (handle, packet_rx, close_rx, claimed) = open_handle();
    claimed.store(true, Ordering::SeqCst);
    (handle, packet_rx, close_rx)
}

#[test]
fn a_fresh_handle_is_closed() {
    let handle = ChannelHandle::new();
    let status = handle.status();
    assert_eq!(status.phase, ChannelPhase::Closed);
    assert!(status.url.is_none());
    assert!(!status.claimed);
}

#[test]
fn install_opens_and_tracks_the_live_claim() {
    let (handle, _packets, _close, claimed) = open_handle();
    let status = handle.status();
    assert_eq!(status.phase, ChannelPhase::Open);
    assert_eq!(status.url.as_deref(), Some("wss://127.0.0.1:1"));
    assert!(!status.claimed, "nothing has connected yet");
    claimed.store(true, Ordering::SeqCst);
    assert!(
        handle.status().claimed,
        "status reads the hub's live claimed flag, not a copy taken at install",
    );
}

#[test]
fn verification_requires_a_claim() {
    assert_eq!(
        ChannelHandle::new().mark_verified(),
        Err(ChannelVerifyError::NotOpen),
    );
    let (handle, _packets, _close, claimed) = open_handle();
    assert_eq!(
        handle.mark_verified(),
        Err(ChannelVerifyError::NotClaimed),
        "verifying with nothing connected has no claim to prove ownership of, and \
         would leave a verified channel whose queue nothing drains",
    );
    assert_eq!(handle.status().phase, ChannelPhase::Open);
    claimed.store(true, Ordering::SeqCst);
    assert_eq!(handle.mark_verified(), Ok(()));
    assert_eq!(handle.status().phase, ChannelPhase::Verified);
}

#[test]
fn emit_is_refused_until_verified_and_the_reasons_are_distinguishable() {
    assert_eq!(
        ChannelHandle::new().emit("x".to_string()),
        Err(ChannelSendError::NotOpen),
        "no channel at all",
    );
    let (handle, mut packets, _close) = claimed_handle();
    assert_eq!(
        handle.emit("x".to_string()),
        Err(ChannelSendError::NotVerified),
        "connected but unproven: telemetry must not flow",
    );
    assert_eq!(handle.mark_verified(), Ok(()));
    assert!(handle.emit("x".to_string()).is_ok());
    assert_eq!(packets.try_recv().ok().as_deref(), Some("x"));
}

#[test]
fn a_control_packet_bypasses_the_verification_gate() {
    let (handle, mut packets, _close, _claimed) = open_handle();
    // The handshake packet is the thing the peer verifies ITSELF by seeing, so it
    // necessarily precedes verification - unlike a body's emit.
    assert!(handle.send_control("greeting".to_string()).is_ok());
    assert_eq!(packets.try_recv().ok().as_deref(), Some("greeting"));
    assert_eq!(
        ChannelHandle::new().send_control("x".to_string()),
        Err(ChannelSendError::NotOpen),
        "a closed channel still refuses control packets",
    );
}

#[test]
fn a_dead_hub_is_reported_as_hub_gone() {
    let (handle, packets, _close) = claimed_handle();
    assert_eq!(handle.mark_verified(), Ok(()));
    drop(packets);
    assert_eq!(handle.emit("x".to_string()), Err(ChannelSendError::HubGone));
    assert_eq!(
        handle.send_control("x".to_string()),
        Err(ChannelSendError::HubGone),
        "a control send detects the gone peer the same way - that is how channel_open \
         learns the connection died",
    );
}

#[test]
fn close_signals_on_its_own_lane_not_the_packet_queue() {
    let (handle, mut packets, mut close, _claimed) = open_handle();
    handle.send_control("queued".to_string()).expect("queued");
    assert!(handle.close(1000, "done"));
    assert_eq!(
        close.try_recv(),
        Ok((1000, "done".to_string())),
        "a planned close must never wait behind traffic, or the agent sees a bare 1006",
    );
    assert_eq!(
        packets.try_recv().ok().as_deref(),
        Some("queued"),
        "and the close does not disturb what was already queued",
    );
    assert_eq!(handle.status().phase, ChannelPhase::Closed);
    assert!(!handle.close(1000, "again"), "closing twice is a no-op");
}

#[test]
fn close_if_unverified_only_fires_while_unproven() {
    let (handle, _packets, _close, _claimed) = open_handle();
    assert!(
        handle.close_if_unverified(1008, "expired"),
        "an open, unverified channel is exactly what the timer tears down",
    );
    assert_eq!(handle.status().phase, ChannelPhase::Closed);

    let (handle, _packets, _close) = claimed_handle();
    assert_eq!(handle.mark_verified(), Ok(()));
    assert!(
        !handle.close_if_unverified(1008, "expired"),
        "verification can land between the sleep elapsing and the task running; the \
         re-check under the lock is what stops the timer killing a proven channel",
    );
    assert_eq!(handle.status().phase, ChannelPhase::Verified);
}

#[test]
fn arming_the_timer_again_stands_the_previous_one_down() {
    let (handle, _packets, _close, _claimed) = open_handle();
    let (first, mut first_rx) = tk::oneshot::channel::<()>();
    handle.arm_verify(first);
    assert!(
        matches!(first_rx.try_recv(), Err(tk::oneshot::error::TryRecvError::Empty)),
        "still armed while the handle holds the sender",
    );
    let (second, _second_rx) = tk::oneshot::channel::<()>();
    handle.arm_verify(second);
    assert!(
        matches!(first_rx.try_recv(), Err(tk::oneshot::error::TryRecvError::Closed)),
        "re-opening drops the previous sender, so two timers never race one channel",
    );
}

#[test]
fn verification_and_close_both_cancel_the_timer() {
    let (handle, _packets, _close) = claimed_handle();
    let (cancel, mut cancel_rx) = tk::oneshot::channel::<()>();
    handle.arm_verify(cancel);
    assert_eq!(handle.mark_verified(), Ok(()));
    assert!(matches!(
        cancel_rx.try_recv(),
        Err(tk::oneshot::error::TryRecvError::Closed),
    ));

    let (handle, _packets, _close, _claimed) = open_handle();
    let (cancel, mut cancel_rx) = tk::oneshot::channel::<()>();
    handle.arm_verify(cancel);
    handle.close(1000, "done");
    assert!(matches!(
        cancel_rx.try_recv(),
        Err(tk::oneshot::error::TryRecvError::Closed),
    ));
}

#[test]
fn escape_line_folds_a_multi_line_render_back_onto_one_line() {
    // Frames BATCH into one client event joined by newlines, so a literal newline in a
    // packet is indistinguishable from a batch boundary.
    assert_eq!(escape_line("a\nb"), "a\\nb");
    assert_eq!(escape_line("a\r\nb"), "a\\r\\nb");
    assert_eq!(escape_line("plain"), "plain");
}

fn an_event() -> nu::Value {
    let span = nu::Span::unknown();
    let mut data = nu::Record::new();
    data.insert(
        "msg",
        nu::Value::string("line one\nline two".to_string(), span),
    );
    nu::Value::record(data, span)
}

fn an_id() -> MsgId {
    mint_msg_id(&NonceGen::new(), "mcp", "channel/Open", "{}", None)
}

#[test]
fn a_rendered_packet_is_one_line_and_parses_back() {
    let event = an_event();
    let line = render_packet(an_id(), "mcp", "channel/Open", &event, None).expect("render");
    assert!(!line.contains('\n'), "a packet must never span lines: {line:?}");

    let value = nu::from_nuon(&line, None).expect("valid NUON record");
    let record = value.as_record().expect("record");
    assert!(
        record.get("id").and_then(|v| v.as_str().ok()).is_some_and(|s| !s.is_empty()),
        "every message carries an id",
    );
    assert_eq!(record.get("from").and_then(|v| v.as_str().ok()), Some("mcp"));
    assert_eq!(
        record.get("model").and_then(|v| v.as_str().ok()),
        Some("channel/Open"),
    );
    assert!(
        record.get("attached").is_none(),
        "attached is omitted when there is nothing attached, never sent empty",
    );
    let event = record.get("event").expect("event").as_record().expect("record");
    assert_eq!(
        event.get("msg").and_then(|v| v.as_str().ok()),
        Some("line one\nline two"),
        "the escape must round-trip to the original string, not merely survive framing",
    );
}

#[test]
fn an_attachment_is_carried_by_name() {
    let line = render_packet(an_id(), "thread/abc", "foo/bar/Car", &an_event(), Some("x.nuon"))
        .expect("render");
    let value = nu::from_nuon(&line, None).expect("valid NUON record");
    let record = value.as_record().expect("record");
    assert_eq!(
        record.get("attached").and_then(|v| v.as_str().ok()),
        Some("x.nuon"),
        "the wire carries the inbox NAME; the content was written under it",
    );
    assert_eq!(
        record.get("from").and_then(|v| v.as_str().ok()),
        Some("thread/abc"),
    );
}

#[test]
fn ids_are_distinct_for_identical_packets() {
    let nonce_gen = NonceGen::new();
    let a = mint_msg_id(&nonce_gen, "mcp", "channel/Open", "{}", None);
    let b = mint_msg_id(&nonce_gen, "mcp", "channel/Open", "{}", None);
    assert_ne!(
        a.to_string(),
        b.to_string(),
        "the generator mixes a counter and a timestamp, so two byte-identical packets \
         still get distinct ids",
    );
    assert!(lib_grammar::is_base62(&a.to_string()), "ids render base62");
}
