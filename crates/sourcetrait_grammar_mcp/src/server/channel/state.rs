use crate::*;

/// The model-path prefix RESERVED for host-originated packets (the_user).
///
/// Every model the HOST stamps lives beneath it - `mcp/channel/Open`,
/// `mcp/supervisor/*`, `mcp/channel/spam/*`. The value of a reservation is that it
/// makes provenance a MECHANICAL check: any model claiming `mcp/` from a source that is
/// not the host can be rejected without interpreting it. Enforced today at the one place
/// a non-host chooses a model (`grimm channel_send`), and available to the mcp-to-mcp
/// peer surface on the same terms.
pub(crate) const MCP_RESERVED_PREFIX: &str = "mcp/";

/// The usable frame maximum (P8).
///
/// A hard `<`, never a `<=` against 1 MiB: a frame landing EXACTLY on the cap arrives
/// missing its first byte - not an error and not a dropped event, but a malformed
/// record that fails `from nuon` at the reader with nothing upstream to blame. Above
/// the cap the frame is dropped whole and the watch closes.
pub(crate) const MAX_FRAME_BYTES: usize = 1_048_575;

/// What an emit is allowed to do right now.
///
/// Read at SEND time, never captured: a job outliving its eval keeps its decl and the
/// state it closed over (P4/P5), so a spawn-time snapshot would let work started before
/// verification emit to an unproven peer - exactly what the gating forbids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChannelPhase {
    /// No hub running. Channels are OPTIONAL and start lazily on the first
    /// `channel_open()`, so this is the ordinary state of a session that never uses
    /// one - an emit here is a plain catchable error, not a fault.
    Closed,
    /// The hub is up but the peer has not yet proven it owns the stdio session.
    Open,
    /// `channel_verified()` succeeded; emits flow.
    Verified,
}

/// What a planned close carries to the hub: the code, the reason, and an OPTIONAL
/// completion sender the hub fires once the frame has actually been flushed.
///
/// The completion half exists for SHUTDOWN. The frame is written by the hub task
/// asynchronously, so a caller that exits the process immediately after asking for a
/// close races that write and leaves the peer with exactly the bare 1006 the explicit
/// close exists to prevent. Awaiting an ack makes the ordering deterministic instead of
/// a sleep long enough to usually work.
pub(crate) type CloseSignal = (u16, String, Option<tk::oneshot::Sender<()>>);

/// A snapshot a caller can hold without keeping the lock.
#[derive(Clone, Debug)]
pub(crate) struct ChannelStatus {
    pub phase: ChannelPhase,
    pub url: Option<String>,
    pub claimed: bool,
}

/// What the spam counter decided about one send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpamVerdict {
    /// Under both thresholds.
    Clear,
    /// Crossed the soft threshold for the FIRST time. Warn once, keep operating - the
    /// agent may react, though nothing depends on it doing so.
    Warn { hits: u32, window_secs: u64 },
    /// Crossed the hard threshold. Refuse the send and stop the offender. `notify` is
    /// true only on the FIRST crossing of an episode: the refusal has to persist for
    /// every later send, but announcing it every time would make the report about spam
    /// into spam itself.
    Stop {
        hits: u32,
        window_secs: u64,
        notify: bool,
    },
}

/// One origin's recent sends, plus whether it has already been warned and stopped in
/// this episode. Both flags reset when the window empties and the origin is evicted, so
/// a producer that goes quiet and later misbehaves again is a fresh episode.
#[derive(Default)]
struct OriginCounter {
    hits: Vec<Instant>,
    warned: bool,
    stopped: bool,
}

struct ChannelInner {
    phase: ChannelPhase,
    url: Option<String>,
    /// The live spam policy. Seeded from CONFIG at construction; `config_channel`
    /// mutates THIS rather than CONFIG, which is set once at startup.
    spam: SpamThresholds,
    /// Recent sends per origin. Bounded by eviction rather than by a cap: an origin
    /// whose window empties is dropped, so this holds only currently-emitting work.
    counters: HashMap<String, OriginCounter>,
    /// Installed by `run_server` after the emergency lane exists. Absent on the
    /// one-shot CLI path, which runs no long-lived tasks.
    emergency: Option<EmergencyTx>,
    /// Rendered, newline-escaped NUON lines, one per packet.
    packets: Option<tk::UnboundedSender<String>>,
    /// The planned-close signal, deliberately NOT sharing the packet queue: a close
    /// must never wait behind traffic, because without an explicit close frame the
    /// agent sees a bare 1006 and cannot tell shutdown from a crash (P11).
    close: Option<tk::oneshot::Sender<CloseSignal>>,
    shutdown: Option<tk::oneshot::Sender<()>>,
    claimed: Option<Arc<AtomicBool>>,
    /// Holding this sender is what keeps the verify timer armed; DROPPING it is the
    /// cancel. Verification, a close, and a re-open all drop it, so a timer can only
    /// ever fire against the open it was armed for.
    verify_cancel: Option<tk::oneshot::Sender<()>>,
    /// The inbox directory, set at open. A DELIBERATE `channel_close()` takes it and
    /// prunes it; every other teardown leaves it, so an attachment stays readable by
    /// nonce when the channel went away without the caller asking for it.
    inbox: Option<PathBuf>,
}

/// Why an emit was refused. The variants are deliberately distinguishable: a consumer
/// must be able to tell "stop working, the channel is gone" from "this send failed",
/// or a background loop will either exit on a blip or spin against a dead channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelSendError {
    /// No channel exists. Terminal for the caller's purposes.
    NotOpen,
    /// Open but unverified - forbidden, and the channel is torn down for trying.
    NotVerified,
    /// The hub task is gone while state still said otherwise. Terminal.
    HubGone,
}

impl ChannelSendError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NotOpen => "the channel is not open; call channel_open() first",
            Self::NotVerified => {
                "the channel is not verified; emitting before channel_verified() is forbidden"
            }
            Self::HubGone => "the channel hub is gone",
        }
    }
}

/// Why a verification was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelVerifyError {
    /// Nothing to verify - `channel_open()` has not run.
    NotOpen,
    /// The hub is up but nothing has connected, so there is no claim to prove
    /// ownership of.
    NotClaimed,
}

/// The host's single channel. One per process: the plan's design is ONE channel whose
/// packets carry their own source, not a channel per producer.
///
/// Guarded by a std Mutex rather than an async one on purpose - `grimm channel_send`
/// runs on the eval thread inside a synchronous nu `Command::run`, and
/// `UnboundedSender::send` is itself sync, so the whole emit path stays lock-cheap and
/// needs no runtime handle.
pub(crate) struct ChannelHandle {
    inner: std::sync::Mutex<ChannelInner>,
    /// Mints every `MsgId` on this channel. Held here rather than passed in, so the
    /// emit path needs nothing threaded to it and all ids share one counter.
    nonce_gen: Arc<NonceGen>,
}

impl ChannelHandle {
    /// Takes its starting policy rather than reading CONFIG, so the handle is
    /// constructible without a process-global - which is what lets the state machine and
    /// the counter be unit-tested at all.
    pub(crate) fn new(spam: SpamThresholds) -> Self {
        Self {
            inner: std::sync::Mutex::new(ChannelInner {
                phase: ChannelPhase::Closed,
                url: None,
                spam,
                counters: HashMap::new(),
                emergency: None,
                packets: None,
                close: None,
                shutdown: None,
                claimed: None,
                verify_cancel: None,
                inbox: None,
            }),
            nonce_gen: Arc::new(NonceGen::new()),
        }
    }

    pub(crate) fn nonce_gen(&self) -> &NonceGen {
        &self.nonce_gen
    }

    /// Where attachments land. Set by `channel_open`, which owns the path and creates
    /// the directory, so the emit path needs no knowledge of the store coordinate.
    pub(crate) fn set_inbox(
        &self,
        dir: PathBuf,
    ) {
        self.lock().inbox = Some(dir);
    }

    pub(crate) fn inbox(&self) -> Option<PathBuf> {
        self.lock().inbox.clone()
    }

    /// Take the inbox path, clearing it - the prune handoff for a DELIBERATE close.
    ///
    /// A caller that closes the channel is declaring it is done with it, which is what
    /// makes dropping that channel's attachments intentional rather than a guess; the
    /// system pruner then has less to do (the_user). Only the `channel_close()` tool
    /// path calls this. A shutdown close, a verify-timer expiry and the
    /// unverified-emit teardown are all HOST-initiated, so they leave the inbox where
    /// it is and an attachment stays readable by nonce.
    ///
    /// Clearing is what makes it safe twice over: a second close cannot prune a
    /// directory a later open recreated.
    pub(crate) fn take_inbox(&self) -> Option<PathBuf> {
        self.lock().inbox.take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ChannelInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn status(&self) -> ChannelStatus {
        let inner = self.lock();
        ChannelStatus {
            phase: inner.phase,
            url: inner.url.clone(),
            claimed: claimed_of(&inner),
        }
    }

    /// Record a freshly bound hub. The phase moves to Open and the verify window starts
    /// at the caller (server/tool/channel_open.rs).
    pub(crate) fn install(
        &self,
        url: String,
        packets: tk::UnboundedSender<String>,
        close: tk::oneshot::Sender<CloseSignal>,
        shutdown: tk::oneshot::Sender<()>,
        claimed: Arc<AtomicBool>,
    ) {
        let mut inner = self.lock();
        inner.phase = ChannelPhase::Open;
        inner.url = Some(url);
        inner.packets = Some(packets);
        inner.close = Some(close);
        inner.shutdown = Some(shutdown);
        inner.claimed = Some(claimed);
    }

    /// Arm the verify timer, standing any previous one down.
    ///
    /// The assignment DROPS the prior sender, which resolves that timer's cancel arm -
    /// so re-opening an already-open channel can never leave two timers racing to tear
    /// one channel down.
    pub(crate) fn arm_verify(
        &self,
        cancel: tk::oneshot::Sender<()>,
    ) {
        self.lock().verify_cancel = Some(cancel);
    }

    /// Promote Open -> Verified, but ONLY once a peer has actually claimed.
    ///
    /// Verifying with nothing connected is meaningless on the handshake's own terms -
    /// the agent proves it owns THE CLAIMING connection - and it would leave a verified
    /// channel whose queue nothing drains, since the receiver is still parked in the
    /// accept loop waiting for a peer.
    pub(crate) fn mark_verified(&self) -> Result<(), ChannelVerifyError> {
        let mut inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return Err(ChannelVerifyError::NotOpen);
        }
        if !claimed_of(&inner) {
            return Err(ChannelVerifyError::NotClaimed);
        }
        inner.phase = ChannelPhase::Verified;
        inner.verify_cancel = None;
        Ok(())
    }

    /// Tear the channel down: ask the peer to close with a reason, then drop the hub.
    ///
    /// The close signal goes first so the client sees WHY; dropping the shutdown sender
    /// is what actually ends the accept loop. Both are best-effort - a peer that already
    /// vanished simply makes the send fail.
    pub(crate) fn close(
        &self,
        code: u16,
        reason: &str,
    ) -> bool {
        let mut inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return false;
        }
        close_locked(&mut inner, code, reason, None);
        true
    }

    /// Close, handing back a receiver that fires once the frame is on the wire.
    ///
    /// For SHUTDOWN, where the process is about to exit and would otherwise race the
    /// hub's write. `None` means there was nothing to close, so there is nothing to
    /// wait for either.
    pub(crate) fn close_and_await(
        &self,
        code: u16,
        reason: &str,
    ) -> Option<tk::oneshot::Receiver<()>> {
        let mut inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return None;
        }
        let (done_tx, done_rx) = tk::oneshot::channel::<()>();
        close_locked(&mut inner, code, reason, Some(done_tx));
        Some(done_rx)
    }

    /// Close ONLY while still unverified - the verify timer's expiry action.
    ///
    /// The phase is re-read here, under the lock, because verification can land between
    /// the timer's sleep elapsing and its task being scheduled; a bare `close` would
    /// then tear down a channel that had just proven itself.
    pub(crate) fn close_if_unverified(
        &self,
        code: u16,
        reason: &str,
    ) -> bool {
        let mut inner = self.lock();
        if !matches!(inner.phase, ChannelPhase::Open) {
            return false;
        }
        close_locked(&mut inner, code, reason, None);
        true
    }

    /// Push a HOST control packet, bypassing the verification gate.
    ///
    /// The gate keeps TELEMETRY off an unverified peer; the handshake packet is the
    /// thing the peer verifies ITSELF by seeing, so it necessarily precedes
    /// verification - the hub sends the same packet on connect for the same reason.
    /// Only a body's `grimm channel_send` goes through `emit`.
    pub(crate) fn send_control(
        &self,
        line: String,
    ) -> Result<(), ChannelSendError> {
        let inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return Err(ChannelSendError::NotOpen);
        }
        push_locked(&inner, line)
    }

    /// The emit path. Phase is re-read here, under the lock, at the moment of sending.
    pub(crate) fn emit(
        &self,
        line: String,
    ) -> Result<(), ChannelSendError> {
        let inner = self.lock();
        match inner.phase {
            ChannelPhase::Closed => Err(ChannelSendError::NotOpen),
            ChannelPhase::Open => Err(ChannelSendError::NotVerified),
            ChannelPhase::Verified => push_locked(&inner, line),
        }
    }
}

impl ChannelHandle {
    /// Install the emergency lane. Separate from construction because the lane is built
    /// in `run_server`, after the handle exists.
    pub(crate) fn install_emergency(
        &self,
        tx: EmergencyTx,
    ) {
        self.lock().emergency = Some(tx);
    }

    pub(crate) fn thresholds(&self) -> SpamThresholds {
        self.lock().spam
    }

    /// Apply a PARTIAL update - only the supplied fields move - and return what is now
    /// in force, so a caller sees the effective policy rather than assuming its own.
    pub(crate) fn set_thresholds(
        &self,
        warn_window_secs: Option<u64>,
        warn_rate: Option<u32>,
        error_window_secs: Option<u64>,
        error_rate: Option<u32>,
    ) -> Result<SpamThresholds, String> {
        let mut inner = self.lock();
        let mut spam = inner.spam;
        if let Some(secs) = warn_window_secs {
            spam.warn_window = positive_window("spam_warn_window_secs", secs)?;
        }
        if let Some(rate) = warn_rate {
            spam.warn_rate = positive_rate("spam_warn_rate", rate)?;
        }
        if let Some(secs) = error_window_secs {
            spam.error_window = positive_window("spam_error_window_secs", secs)?;
        }
        if let Some(rate) = error_rate {
            spam.error_rate = positive_rate("spam_error_rate", rate)?;
        }
        inner.spam = spam;
        Ok(spam)
    }

    /// Record one send by `from` and say what it costs.
    ///
    /// Counting keys on the ORIGIN, which is host-stamped, so a body cannot spread its
    /// traffic across identities to stay under the rate. Every origin's window is pruned
    /// on each call and an origin whose window empties is EVICTED, so the map holds only
    /// currently-emitting work rather than one entry per origin for the host's life.
    pub(crate) fn record_send(
        &self,
        from: &str,
    ) -> SpamVerdict {
        self.record_send_at(from, Instant::now())
    }

    /// The counting itself, with `now` injected so it is unit-testable without sleeping -
    /// the same shape `scan_hung` uses for the watchdog.
    pub(crate) fn record_send_at(
        &self,
        from: &str,
        now: Instant,
    ) -> SpamVerdict {
        let mut inner = self.lock();
        let spam = inner.spam;
        let retention = spam.retention();
        inner.counters.retain(|_, counter| {
            counter
                .hits
                .retain(|at| now.saturating_duration_since(*at) < retention);
            !counter.hits.is_empty()
        });
        let counter = inner.counters.entry(from.to_string()).or_default();
        counter.hits.push(now);
        let within = |window: tk::TkDuration| -> u32 {
            counter
                .hits
                .iter()
                .filter(|at| now.saturating_duration_since(**at) < window)
                .count() as u32
        };
        let errors = within(spam.error_window);
        if errors >= spam.error_rate {
            let notify = !counter.stopped;
            counter.stopped = true;
            return SpamVerdict::Stop {
                hits: errors,
                window_secs: spam.error_window.as_secs(),
                notify,
            };
        }
        let warns = within(spam.warn_window);
        if warns >= spam.warn_rate && !counter.warned {
            counter.warned = true;
            return SpamVerdict::Warn {
                hits: warns,
                window_secs: spam.warn_window.as_secs(),
            };
        }
        SpamVerdict::Clear
    }

    /// Push an Emergency onto the internal lane, if one is installed.
    pub(crate) fn fire_emergency(
        &self,
        emergency: Emergency,
    ) {
        if let Some(tx) = &self.lock().emergency {
            let _ = tx.send(emergency);
        }
    }
}

fn positive_window(
    name: &str,
    secs: u64,
) -> Result<tk::TkDuration, String> {
    if secs == 0 {
        Err(format!("{name} must be a positive number of seconds"))
    } else {
        Ok(tk::TkDuration::from_secs(secs))
    }
}

fn positive_rate(
    name: &str,
    rate: u32,
) -> Result<u32, String> {
    if rate == 0 {
        Err(format!("{name} must be greater than zero"))
    } else {
        Ok(rate)
    }
}

/// The host's ONE channel, reachable from an eval.
///
/// A process-global rather than a threaded parameter: the channel is one-per-process by
/// design, exactly like CONFIG, and the alternative is passing an `Arc<ChannelHandle>`
/// through NuSh -> dispatch -> eval_stateless / InteractEngine -> eval_in_process ->
/// register_nuapi, five signatures on the hot eval path, for a value that can never vary
/// per eval. `NuSh` reads the same handle rather than owning its own, so the two can
/// never diverge.
static CHANNEL: OnceLock<Arc<ChannelHandle>> = OnceLock::new();

pub(crate) fn channel_handle() -> Arc<ChannelHandle> {
    CHANNEL
        .get_or_init(|| Arc::new(ChannelHandle::new(config().channel.spam)))
        .clone()
}

fn claimed_of(inner: &ChannelInner) -> bool {
    inner
        .claimed
        .as_ref()
        .map(|c| c.load(Ordering::SeqCst))
        .unwrap_or(false)
}

fn push_locked(
    inner: &ChannelInner,
    line: String,
) -> Result<(), ChannelSendError> {
    match &inner.packets {
        Some(tx) => tx.send(line).map_err(|_| ChannelSendError::HubGone),
        None => Err(ChannelSendError::HubGone),
    }
}

/// The teardown itself, with the lock already held, so the two entry points that decide
/// WHETHER to close cannot drift about WHAT closing does.
fn close_locked(
    inner: &mut ChannelInner,
    code: u16,
    reason: &str,
    done: Option<tk::oneshot::Sender<()>>,
) {
    if let Some(tx) = inner.close.take() {
        let _ = tx.send((code, reason.to_string(), done));
    }
    inner.phase = ChannelPhase::Closed;
    inner.url = None;
    inner.packets = None;
    inner.claimed = None;
    // Dropping the oneshot sender signals the hub task even if nothing is received.
    inner.shutdown = None;
    // Likewise the verify sender: a closed channel has nothing left to verify.
    inner.verify_cancel = None;
}

/// A channel message's id.
///
/// Distinct from `Nonce` in NAME rather than in shape: nonces name evals throughout
/// this crate, so reusing the word on the wire would confuse two different things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MsgId(Nonce);

impl Display for MsgId {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Mint a packet's id over the packet's own fields.
///
/// The ATTACHED CONTENT is hashed, never its path: the path is derived FROM the id, so
/// hashing it would be circular. The generator mixes in a counter and a timestamp, so
/// two byte-identical packets still get distinct ids.
pub(crate) fn mint_msg_id(
    nonce_gen: &NonceGen,
    from: &str,
    model: &str,
    event_nuon: &str,
    attached_nuon: Option<&str>,
) -> MsgId {
    MsgId(nonce_gen.next(&(from, model, event_nuon, attached_nuon)))
}

/// Render a value as compact NUON - the form both the wire and the id-hash see.
pub(crate) fn render_nuon(value: &nu::Value) -> Result<String, String> {
    nu::to_nuon(&nu::EngineState::new(), value, nu::ToNuonConfig::default())
        .map_err(|e| e.to_string())
}

/// Render one packet as the single NUON line the wire carries.
///
/// `id` and `from` are stamped by the caller on the HOST side and never taken from a
/// body, so a body cannot forge attribution. `attached` is a name, not content - the
/// content was already written to the inbox under that name.
pub(crate) fn render_packet(
    id: MsgId,
    from: &str,
    model: &str,
    event: &nu::Value,
    attached: Option<&str>,
) -> Result<String, String> {
    let span = nu::Span::unknown();
    let mut record = nu::Record::new();
    record.insert("id", nu::Value::string(id.to_string(), span));
    record.insert("from", nu::Value::string(from.to_string(), span));
    record.insert("model", nu::Value::string(model.to_string(), span));
    record.insert("event", event.clone());
    if let Some(name) = attached {
        record.insert("attached", nu::Value::string(name.to_string(), span));
    }
    let rendered = render_nuon(&nu::Value::record(record, span))?;
    Ok(escape_line(&rendered))
}

/// Re-escape raw newlines so one packet is always one line.
///
/// `to nuon` renders compactly but does NOT escape a newline INSIDE a string value, and
/// the client BATCHES frames arriving close together into one event joined by newlines
/// (P9). So a literal newline in a packet is indistinguishable from a batch boundary and
/// a reader would see more records than were sent. Safe because the only raw newlines a
/// compact render can carry are inside double-quoted strings, where `\n` / `\r` ARE the
/// escapes nushell reads back - the line still parses to the original value.
pub(crate) fn escape_line(rendered: &str) -> String {
    rendered.replace('\n', "\\n").replace('\r', "\\r")
}
