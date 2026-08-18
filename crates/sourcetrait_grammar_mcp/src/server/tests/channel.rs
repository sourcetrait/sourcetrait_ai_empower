use crate::*;
use crate::server::channel::state::{MsgId, escape_line};

/// A permissive starting policy, so a test that is not ABOUT the thresholds never trips
/// them. The ones that are set their own explicitly, rather than leaning on the shipped
/// defaults - those are INITIAL values and expected to move.
fn test_policy() -> SpamThresholds {
    SpamThresholds {
        warn_window: tk::TkDuration::from_secs(10),
        warn_rate: 1_000,
        error_window: tk::TkDuration::from_secs(10),
        error_rate: 10_000,
    }
}

/// Install a hub-shaped channel without a hub: the handle only ever holds the two
/// senders, the shutdown oneshot and the claimed flag, so the state machine is testable
/// without binding a socket or reading a certificate.
fn open_handle() -> (
    ChannelHandle,
    tk::UnboundedReceiver<String>,
    tk::oneshot::Receiver<ChannelCloseSignal>,
    Arc<AtomicBool>,
) {
    let handle = ChannelHandle::new(test_policy());
    let (packets, packet_rx) = tk::unbounded_channel::<String>();
    let (close, close_rx) = tk::oneshot::channel::<ChannelCloseSignal>();
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
    tk::oneshot::Receiver<ChannelCloseSignal>,
) {
    let (handle, packet_rx, close_rx, claimed) = open_handle();
    claimed.store(true, Ordering::SeqCst);
    (handle, packet_rx, close_rx)
}

#[test]
fn a_fresh_handle_is_closed() {
    let handle = ChannelHandle::new(test_policy());
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
        ChannelHandle::new(test_policy()).mark_verified(),
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
        ChannelHandle::new(test_policy()).emit("x".to_string()),
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
        ChannelHandle::new(test_policy()).send_control("x".to_string()),
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
    let (code, reason, done) = close.try_recv().expect("the close rides its own lane");
    assert_eq!(
        (code, reason.as_str()),
        (1000, "done"),
        "a planned close must never wait behind traffic, or the agent sees a bare 1006",
    );
    assert!(done.is_none(), "a plain close asks for no flush ack");
    assert_eq!(
        packets.try_recv().ok().as_deref(),
        Some("queued"),
        "and the close does not disturb what was already queued",
    );
    assert_eq!(handle.status().phase, ChannelPhase::Closed);
    assert!(!handle.close(1000, "again"), "closing twice is a no-op");
}

#[test]
fn close_and_await_hands_back_a_flush_ack() {
    let (handle, _packets, mut close, _claimed) = open_handle();
    let mut done_rx = handle
        .close_and_await(1001, "host shutting down")
        .expect("an open channel has something to close");
    let (code, reason, done_tx) = close.try_recv().expect("the close was signalled");
    assert_eq!((code, reason.as_str()), (1001, "host shutting down"));
    // The hub fires this AFTER flushing the frame, which is what lets a shutting-down
    // host wait for the close to reach the wire instead of racing its own exit and
    // leaving the peer a bare 1006.
    done_tx
        .expect("a shutdown close carries an ack sender")
        .send(())
        .expect("ack");
    assert!(done_rx.try_recv().is_ok(), "the waiter observes the flush");
}

#[test]
fn close_and_await_on_a_closed_channel_has_nothing_to_wait_for() {
    assert!(
        ChannelHandle::new(test_policy())
            .close_and_await(1001, "host shutting down")
            .is_none(),
        "no channel means no frame to wait for, so shutdown must not block on one",
    );
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
fn a_deliberate_close_takes_the_inbox_to_prune_it() {
    let handle = ChannelHandle::new(test_policy());
    assert!(
        handle.take_inbox().is_none(),
        "a channel that never opened wrote nothing, so there is nothing to prune",
    );
    let dir = PathBuf::from("/dev/shm/box/mcp/nom/inbox");
    handle.set_inbox(dir.clone());
    assert_eq!(handle.inbox(), Some(dir.clone()), "open records where attachments go");
    assert_eq!(
        handle.take_inbox(),
        Some(dir),
        "the deliberate-close path takes the path so it can prune it",
    );
    assert!(
        handle.take_inbox().is_none(),
        "taking CLEARS it, so a second close cannot prune a dir a later open recreated",
    );
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
    mint_msg_id(&datum::NonceGenerator::new(), "mcp", "mcp/channel/Open", "{}", None)
}

#[test]
fn a_rendered_packet_is_one_line_and_parses_back() {
    let event = an_event();
    let line = render_packet(an_id(), "mcp", "mcp/channel/Open", &event, None).expect("render");
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
        Some("mcp/channel/Open"),
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

/// A tight, explicit policy so the tests do not depend on the shipped defaults - which
/// are INITIAL values and expected to move once production says what a normal producer
/// does.
fn with_policy(
    handle: &ChannelHandle,
    warn_rate: u32,
    error_rate: u32,
) {
    handle
        .set_thresholds(Some(10), Some(warn_rate), Some(10), Some(error_rate))
        .expect("valid policy");
}

#[test]
fn the_soft_threshold_warns_exactly_once_per_origin() {
    let handle = ChannelHandle::new(test_policy());
    with_policy(&handle, 3, 100);
    let now = Instant::now();
    assert_eq!(handle.record_send_at("thread/a", now), SpamVerdict::Clear);
    assert_eq!(handle.record_send_at("thread/a", now), SpamVerdict::Clear);
    assert!(
        matches!(handle.record_send_at("thread/a", now), SpamVerdict::Warn { hits: 3, .. }),
        "the third send reaches the rate",
    );
    assert_eq!(
        handle.record_send_at("thread/a", now),
        SpamVerdict::Clear,
        "ONE warning per abuser - a report about spam must not itself become spam",
    );
}

#[test]
fn the_hard_threshold_stops_and_outranks_the_warning() {
    let handle = ChannelHandle::new(test_policy());
    with_policy(&handle, 2, 3);
    let now = Instant::now();
    handle.record_send_at("thread/a", now);
    assert!(matches!(handle.record_send_at("thread/a", now), SpamVerdict::Warn { .. }));
    assert!(
        matches!(handle.record_send_at("thread/a", now), SpamVerdict::Stop { hits: 3, .. }),
        "the hard threshold is checked first, so crossing both reports the stop",
    );
}

#[test]
fn origins_are_counted_separately() {
    let handle = ChannelHandle::new(test_policy());
    with_policy(&handle, 2, 100);
    let now = Instant::now();
    handle.record_send_at("thread/a", now);
    assert_eq!(
        handle.record_send_at("thread/b", now),
        SpamVerdict::Clear,
        "one origin's traffic must not flag another's",
    );
    assert!(matches!(handle.record_send_at("thread/a", now), SpamVerdict::Warn { .. }));
}

#[test]
fn hits_outside_the_window_stop_counting_and_the_origin_is_evicted() {
    let handle = ChannelHandle::new(test_policy());
    with_policy(&handle, 3, 100);
    let start = Instant::now();
    handle.record_send_at("thread/a", start);
    handle.record_send_at("thread/a", start);
    // Past the 10s window, so the two earlier hits no longer count and this reads as a
    // first send rather than a third.
    let later = start + tk::TkDuration::from_secs(11);
    assert_eq!(handle.record_send_at("thread/a", later), SpamVerdict::Clear);
    assert_eq!(
        handle.record_send_at("thread/a", later),
        SpamVerdict::Clear,
        "and the window really did reset rather than merely skipping one",
    );
}

#[test]
fn thresholds_update_partially_and_reject_zero() {
    let handle = ChannelHandle::new(test_policy());
    with_policy(&handle, 5, 9);
    let updated = handle
        .set_thresholds(None, Some(7), None, None)
        .expect("partial update");
    assert_eq!(updated.warn_rate, 7, "the supplied field moves");
    assert_eq!(updated.error_rate, 9, "and an omitted one does not");
    assert_eq!(updated.warn_window.as_secs(), 10);
    assert!(
        handle.set_thresholds(None, Some(0), None, None).is_err(),
        "a zero rate would forbid the first send rather than police a rate",
    );
    assert!(
        handle.set_thresholds(Some(0), None, None, None).is_err(),
        "a zero window is not a rate at all",
    );
    assert_eq!(
        handle.thresholds().warn_rate,
        7,
        "a rejected update must not have partially applied",
    );
}

#[test]
fn ids_are_distinct_for_identical_packets() {
    let nonce_gen = datum::NonceGenerator::new();
    let a = mint_msg_id(&nonce_gen, "mcp", "mcp/channel/Open", "{}", None);
    let b = mint_msg_id(&nonce_gen, "mcp", "mcp/channel/Open", "{}", None);
    assert_ne!(
        a.to_string(),
        b.to_string(),
        "the generator mixes a counter and a timestamp, so two byte-identical packets \
         still get distinct ids",
    );
    assert!(lib_grammar::is_base62(&a.to_string()), "ids render base62");
}
