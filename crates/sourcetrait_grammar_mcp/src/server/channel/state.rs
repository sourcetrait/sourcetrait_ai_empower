use crate::*;

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

/// A snapshot a caller can hold without keeping the lock.
#[derive(Clone, Debug)]
pub(crate) struct ChannelStatus {
    pub phase: ChannelPhase,
    pub url: Option<String>,
    pub claimed: bool,
}

/// What the hub task accepts over its command channel.
pub(crate) enum HubCommand {
    /// One rendered, newline-escaped NUON line to frame as a single text message.
    Packet(String),
    /// Close the peer connection deliberately, with a code and a reason the client
    /// surfaces verbatim (P3/P11). Planned teardown MUST use this - a dropped socket
    /// reaches the agent as a bare 1006, indistinguishable from a crash.
    Close { code: u16, reason: String },
}

struct ChannelInner {
    phase: ChannelPhase,
    url: Option<String>,
    tx: Option<tk::UnboundedSender<HubCommand>>,
    shutdown: Option<tk::oneshot::Sender<()>>,
    claimed: Option<Arc<AtomicBool>>,
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

/// The host's single channel. One per process: the plan's design is ONE channel whose
/// packets carry their own source, not a channel per producer.
///
/// Guarded by a std Mutex rather than an async one on purpose - `grimm channel_send`
/// runs on the eval thread inside a synchronous nu `Command::run`, and
/// `UnboundedSender::send` is itself sync, so the whole emit path stays lock-cheap and
/// needs no runtime handle.
pub(crate) struct ChannelHandle {
    inner: std::sync::Mutex<ChannelInner>,
}

impl Default for ChannelHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelHandle {
    pub(crate) fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(ChannelInner {
                phase: ChannelPhase::Closed,
                url: None,
                tx: None,
                shutdown: None,
                claimed: None,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ChannelInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn status(&self) -> ChannelStatus {
        let inner = self.lock();
        ChannelStatus {
            phase: inner.phase,
            url: inner.url.clone(),
            claimed: inner
                .claimed
                .as_ref()
                .map(|c| c.load(Ordering::SeqCst))
                .unwrap_or(false),
        }
    }

    /// Record a freshly bound hub. The phase moves to Open and the verify window starts
    /// at the caller (server/tool/channel_open.rs).
    pub(crate) fn install(
        &self,
        url: String,
        tx: tk::UnboundedSender<HubCommand>,
        shutdown: tk::oneshot::Sender<()>,
        claimed: Arc<AtomicBool>,
    ) {
        let mut inner = self.lock();
        inner.phase = ChannelPhase::Open;
        inner.url = Some(url);
        inner.tx = Some(tx);
        inner.shutdown = Some(shutdown);
        inner.claimed = Some(claimed);
    }

    /// Promote Open -> Verified. False when there is no open channel to promote.
    pub(crate) fn mark_verified(&self) -> bool {
        let mut inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return false;
        }
        inner.phase = ChannelPhase::Verified;
        true
    }

    /// Tear the channel down: ask the peer to close with a reason, then drop the hub.
    ///
    /// The close command goes first so the client sees WHY; dropping the shutdown
    /// sender is what actually ends the accept loop. Both are best-effort - a peer that
    /// already vanished simply makes the send fail.
    pub(crate) fn close(
        &self,
        code: u16,
        reason: &str,
    ) -> bool {
        let mut inner = self.lock();
        if matches!(inner.phase, ChannelPhase::Closed) {
            return false;
        }
        if let Some(tx) = &inner.tx {
            let _ = tx.send(HubCommand::Close {
                code,
                reason: reason.to_string(),
            });
        }
        inner.phase = ChannelPhase::Closed;
        inner.url = None;
        inner.tx = None;
        inner.claimed = None;
        // Dropping the oneshot sender signals the hub task even if nothing is received.
        inner.shutdown = None;
        true
    }

    /// The emit path. Phase is re-read here, under the lock, at the moment of sending.
    pub(crate) fn emit(&self, line: String) -> Result<(), ChannelSendError> {
        let inner = self.lock();
        match inner.phase {
            ChannelPhase::Closed => Err(ChannelSendError::NotOpen),
            ChannelPhase::Open => Err(ChannelSendError::NotVerified),
            ChannelPhase::Verified => match &inner.tx {
                Some(tx) => tx
                    .send(HubCommand::Packet(line))
                    .map_err(|_| ChannelSendError::HubGone),
                None => Err(ChannelSendError::HubGone),
            },
        }
    }
}

/// Render one packet as the single NUON line the wire carries.
///
/// `from` is stamped by the caller (host-side) and never taken from a body, so a body
/// cannot forge attribution.
pub(crate) fn render_packet(
    nom: &str,
    from: &str,
    kind: &str,
    data: nu::Value,
) -> Result<String, String> {
    let span = nu::Span::unknown();
    let mut record = nu::Record::new();
    record.insert("nom", nu::Value::string(nom.to_string(), span));
    record.insert("from", nu::Value::string(from.to_string(), span));
    record.insert("kind", nu::Value::string(kind.to_string(), span));
    record.insert("data", data);
    let rendered = nu::to_nuon(
        &nu::EngineState::new(),
        &nu::Value::record(record, span),
        nu::ToNuonConfig::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(escape_line(&rendered))
}

/// Re-escape raw newlines so one packet is always one line.
///
/// `to nuon` renders compactly but does NOT escape a newline INSIDE a string value, and
/// the client BATCHES frames arriving close together into one event joined by newlines
/// (P9). So a literal newline in a packet is indistinguishable from a batch boundary and
/// a reader would see more records than were sent. Safe because the only raw newlines a
/// compact render can carry are inside double-quoted strings, where `\n` / `\r` are the
/// escapes nushell reads back - the line still parses to the original value.
pub(crate) fn escape_line(rendered: &str) -> String {
    rendered.replace('\n', "\\n").replace('\r', "\\r")
}
