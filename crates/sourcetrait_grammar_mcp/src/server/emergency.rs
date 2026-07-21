use crate::*;

// The internal emergency lane (EmbedEngine Phase 5, CLASSIFY-FIRST). The
// watchdog (server/watchdog.rs) DETECTS + CLASSIFIES resource trouble into an
// `Emergency` and pushes it onto an internal MPSC; one `EmergencyResponder`
// consumes. Detection is decoupled from response, and we act on internal state
// INTERNALLY - never by relying on a transmit-out (the #35 channel is a future
// consumer, not a dependency). The responder's ONLY action FOR NOW is to LOG
// each Emergency to `emergency.nuonl`: no restart, no notify, no targeted
// recovery. Those RESPONSES are deferred until every campaign phase is done, so
// we gather data on which conditions actually fire before designing them.

/// Producer end of the EmergencyChannel. Unbounded so a producer (the watchdog)
/// never blocks / back-pressures - it must stay schedulable even under total
/// eval saturation, and the volume is low (the watchdog emits edge-triggered).
pub(crate) type EmergencyTx = tk::UnboundedSender<Emergency>;
/// Consumer end, drained by the sole `EmergencyResponder`.
pub(crate) type EmergencyRx = tk::UnboundedReceiver<Emergency>;

/// Copy discriminant mirror of `Emergency` (the enum-kind-mirror convention):
/// the bare classification without the payload, for the log `kind` field and the
/// watchdog's edge-dedup keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EmergencyKind {
    HungEngineThread,
    CpuWarning,
    RamWarning,
    VramWarning,
    BackgroundJobsWarning,
    DiskWarning,
    ChannelSpamWarning,
    ChannelSpamError,
    Critical,
}

impl EmergencyKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::HungEngineThread => "hung_engine_thread",
            Self::CpuWarning => "cpu_warning",
            Self::RamWarning => "ram_warning",
            Self::VramWarning => "vram_warning",
            Self::BackgroundJobsWarning => "background_jobs_warning",
            Self::DiskWarning => "disk_warning",
            Self::ChannelSpamWarning => "channel_spam_warning",
            Self::ChannelSpamError => "channel_spam_error",
            Self::Critical => "critical",
        }
    }
}

/// An engine thread whose cancel `Signals` was triggered (timeout / kill) but
/// which is still alive past the grace window - the accepted-residual HUNG ENGINE
/// THREAD. It runs the nushell engine on a dedicated blocking thread that will
/// not stop (stuck in pure-Rust where it never polls Signals), and you cannot
/// SIGKILL a thread. `lane` is "stateless" (a pooled run/rerun/call eval,
/// holding a concurrency permit) or "interact" (the single serial lane, the
/// whole stateful engine deadlocked - followup #41). `hung_ms` is time since the
/// cancel; `pool_held` / `pool_cap` snapshot the stateless-pool pressure.
#[derive(Clone, Debug)]
pub(crate) struct HungEngineThreadEmergency {
    pub nonce: String,
    pub lane: String,
    pub tool: String,
    pub started_at: u64,
    pub cancelled_at: u64,
    pub hung_ms: u64,
    pub pool_held: usize,
    pub pool_cap: usize,
}

/// Sustained host-process CPU over the sample window (all threads; can exceed
/// 100 on multi-core). Ambiguous alone - a legit heavy transform looks the same -
/// so it is DATA, not an action trigger, which is what the Warning suffix marks.
#[derive(Clone, Debug)]
pub(crate) struct CpuWarningEmergency {
    pub cpu_pct: f64,
    pub sample_ms: u64,
}

/// Host-process resident set size (VmRSS) above the generous threshold. Data.
#[derive(Clone, Debug)]
pub(crate) struct RamWarningEmergency {
    pub rss_kb: u64,
}

/// GPU memory in use above the threshold fraction (summed across GPUs, from
/// nvidia-smi). Data - attribution to our eval children is a later refinement.
#[derive(Clone, Debug)]
pub(crate) struct VramWarningEmergency {
    pub used_mib: u64,
    pub total_mib: u64,
}

/// The host-owned environment-wide jobs table (P0.8) above the threshold count.
/// A body's `job spawn` persists past its eval; an unbounded accrual is the
/// signal. Data.
#[derive(Clone, Debug)]
pub(crate) struct BackgroundJobsWarningEmergency {
    pub job_count: usize,
}

/// A watched filesystem reached the warning line. Data, like the rest of the Warning
/// family - the system supplies the actual error (ENOSPC) if it ever fills.
#[derive(Clone, Debug)]
pub(crate) struct DiskWarningEmergency {
    pub mount: String,
    pub used_pct: u32,
    pub threshold_pct: u32,
}

/// An origin crossed the SOFT send threshold on the channel. Fired ONCE per origin, so
/// the report about spam never becomes spam itself. It names WHO and how fast, and
/// deliberately carries NO payload example - the agent is already being spammed by that,
/// and can investigate the cause itself.
#[derive(Clone, Debug)]
pub(crate) struct ChannelSpamWarningEmergency {
    pub from: String,
    pub hits: u32,
    pub window_secs: u64,
    pub rate: u32,
}

/// An origin crossed the HARD threshold and was STOPPED. `action` records what the host
/// actually did about it, since the lever differs between a foreground eval and a job
/// that outlived its own.
#[derive(Clone, Debug)]
pub(crate) struct ChannelSpamErrorEmergency {
    pub from: String,
    pub hits: u32,
    pub window_secs: u64,
    pub rate: u32,
    pub action: String,
}

/// The unambiguous total-failure case: something is 100% wrong and the
/// guaranteed-correct response is an MCP restart (the restart-of-last-resort - a
/// Critical restart is INTENDED host-teardown, the legitimate counterpart to the
/// shadowed body-`exit`). The RESPONSE is deferred; for now Critical only logs,
/// like every other variant.
#[derive(Clone, Debug)]
pub(crate) struct CriticalEmergency {
    pub reason: String,
    pub hung: usize,
    pub cap: usize,
}

/// A classified resource condition, produced by the watchdog and consumed by the
/// responder. Fieldful `+Clone` with the mirrored `+Copy EmergencyKind`.
#[derive(Clone, Debug)]
pub(crate) enum Emergency {
    HungEngineThread(HungEngineThreadEmergency),
    CpuWarning(CpuWarningEmergency),
    RamWarning(RamWarningEmergency),
    VramWarning(VramWarningEmergency),
    BackgroundJobsWarning(BackgroundJobsWarningEmergency),
    DiskWarning(DiskWarningEmergency),
    ChannelSpamWarning(ChannelSpamWarningEmergency),
    ChannelSpamError(ChannelSpamErrorEmergency),
    Critical(CriticalEmergency),
}

impl Emergency {
    pub(crate) fn kind(&self) -> EmergencyKind {
        match self {
            Self::HungEngineThread(_) => EmergencyKind::HungEngineThread,
            Self::CpuWarning(_) => EmergencyKind::CpuWarning,
            Self::RamWarning(_) => EmergencyKind::RamWarning,
            Self::VramWarning(_) => EmergencyKind::VramWarning,
            Self::BackgroundJobsWarning(_) => EmergencyKind::BackgroundJobsWarning,
            Self::DiskWarning(_) => EmergencyKind::DiskWarning,
            Self::ChannelSpamWarning(_) => EmergencyKind::ChannelSpamWarning,
            Self::ChannelSpamError(_) => EmergencyKind::ChannelSpamError,
            Self::Critical(_) => EmergencyKind::Critical,
        }
    }

    /// The RESERVED model path this condition rides under on the channel.
    ///
    /// `mcp/` is a RESERVATION (the_user): every host-originated model lives beneath
    /// it, which is what lets a model path from a FOREIGN source be checked
    /// mechanically - anything claiming `mcp/` is not entitled to it. The reservation
    /// is enforced today at the one place a non-host picks a model, `grimm
    /// channel_send`, and will serve the mcp-to-mcp peer surface the same way.
    pub(crate) fn model(&self) -> &'static str {
        match self.kind() {
            EmergencyKind::HungEngineThread => "mcp/supervisor/HungEngineThread",
            EmergencyKind::CpuWarning => "mcp/supervisor/CpuWarning",
            EmergencyKind::RamWarning => "mcp/supervisor/RamWarning",
            EmergencyKind::VramWarning => "mcp/supervisor/VramWarning",
            EmergencyKind::BackgroundJobsWarning => "mcp/supervisor/BackgroundJobsWarning",
            EmergencyKind::DiskWarning => "mcp/supervisor/DiskWarning",
            EmergencyKind::ChannelSpamWarning => "mcp/channel/spam/Warning",
            EmergencyKind::ChannelSpamError => "mcp/channel/spam/Error",
            EmergencyKind::Critical => "mcp/supervisor/Critical",
        }
    }

    /// The variant's OWN fields, without the envelope the log adds.
    ///
    /// Shared by the log line and the channel packet, so the durable record and the
    /// notification can never disagree about what a condition reported.
    fn event_record(&self) -> nu::Record {
        let span = nu::Span::unknown();
        let mut r = nu::Record::new();
        match self {
            Self::HungEngineThread(h) => {
                r.insert("nonce", nu::Value::string(h.nonce.clone(), span));
                r.insert("lane", nu::Value::string(h.lane.clone(), span));
                r.insert("tool", nu::Value::string(h.tool.clone(), span));
                r.insert("started_at", nu::Value::int(h.started_at as i64, span));
                r.insert("cancelled_at", nu::Value::int(h.cancelled_at as i64, span));
                r.insert("hung_ms", nu::Value::int(h.hung_ms as i64, span));
                r.insert("pool_held", nu::Value::int(h.pool_held as i64, span));
                r.insert("pool_cap", nu::Value::int(h.pool_cap as i64, span));
            }
            Self::CpuWarning(c) => {
                r.insert("cpu_pct", nu::Value::float(c.cpu_pct, span));
                r.insert("sample_ms", nu::Value::int(c.sample_ms as i64, span));
            }
            Self::RamWarning(m) => {
                r.insert("rss_kb", nu::Value::int(m.rss_kb as i64, span));
            }
            Self::VramWarning(v) => {
                r.insert("used_mib", nu::Value::int(v.used_mib as i64, span));
                r.insert("total_mib", nu::Value::int(v.total_mib as i64, span));
            }
            Self::BackgroundJobsWarning(b) => {
                r.insert("job_count", nu::Value::int(b.job_count as i64, span));
            }
            Self::DiskWarning(d) => {
                r.insert("mount", nu::Value::string(d.mount.clone(), span));
                r.insert("used_pct", nu::Value::int(d.used_pct as i64, span));
                r.insert("threshold_pct", nu::Value::int(d.threshold_pct as i64, span));
            }
            Self::ChannelSpamWarning(w) => {
                r.insert("from", nu::Value::string(w.from.clone(), span));
                r.insert("hits", nu::Value::int(w.hits as i64, span));
                r.insert("window_secs", nu::Value::int(w.window_secs as i64, span));
                r.insert("rate", nu::Value::int(w.rate as i64, span));
            }
            Self::ChannelSpamError(e) => {
                r.insert("from", nu::Value::string(e.from.clone(), span));
                r.insert("hits", nu::Value::int(e.hits as i64, span));
                r.insert("window_secs", nu::Value::int(e.window_secs as i64, span));
                r.insert("rate", nu::Value::int(e.rate as i64, span));
                r.insert("action", nu::Value::string(e.action.clone(), span));
            }
            Self::Critical(c) => {
                r.insert("reason", nu::Value::string(c.reason.clone(), span));
                r.insert("hung", nu::Value::int(c.hung as i64, span));
                r.insert("cap", nu::Value::int(c.cap as i64, span));
            }
        }
        r
    }

    /// Render this Emergency as a SINGLE-LINE NUON record - one `emergency.nuonl`
    /// line. Every field is a flat scalar (int / float / string with no embedded
    /// newlines), so the record never spans lines. `ts` is the responder's
    /// log-write time (ms since epoch); `mcp_nom` namespaces the log per process.
    pub(crate) fn to_nuon_line(
        &self,
        ts: u64,
        mcp_nom: &str,
    ) -> Result<String, String> {
        let span = nu::Span::unknown();
        let mut r = nu::Record::new();
        r.insert("ts", nu::Value::int(ts as i64, span));
        r.insert("mcp_nom", nu::Value::string(mcp_nom.to_string(), span));
        r.insert("kind", nu::Value::string(self.kind().name().to_string(), span));
        for (key, value) in self.event_record() {
            r.insert(key, value);
        }
        nu::to_nuon(
            &nu::EngineState::new(),
            &nu::Value::record(r, span),
            nu::ToNuonConfig::default(),
        )
        .map_err(|e| e.to_string())
    }
}

/// The per-process emergency log: `<cache>/log/<mcp_nom>/emergency.nuonl`
/// (nuonl = newline-delimited NUON records). `mcp_nom` is the per-process base62
/// id minted at startup, so concurrent MCP hosts on one store coordinate never
/// clobber each other's log.
pub(crate) fn emergency_log_path(mcp_nom: &str) -> PathBuf {
    cache_base_dir().join("log").join(mcp_nom).join("emergency.nuonl")
}

/// Append one already-rendered NUON line (a trailing newline is added). Creates
/// the parent dir on demand; open-append-close per line (low volume, robust
/// against external truncation).
pub(crate) fn append_line(
    path: &std::path::Path,
    line: &str,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Put one Emergency on the channel as a packet under its reserved `mcp/` model.
///
/// THIS IS WHY THE WARNING FAMILY EXISTS AT ALL. Those conditions are notice BEFORE
/// the system's own error arrives, so the agent can act while it still has room - and
/// a line in a log file nobody reads cannot deliver notice. The durable record stays
/// the log; this is the notification.
///
/// Through `emit`, so it is VERIFICATION-GATED like any other telemetry: an unproven
/// peer must not receive host state. A closed or unverified channel simply drops it,
/// which is correct rather than an error - channels are OPTIONAL, and the log already
/// holds the record. No rate limiting is applied or needed: the `LevelGate` upstream
/// already bounds each condition to roughly one report per episode.
fn announce(em: &Emergency) {
    let channel = channel_handle();
    let event = nu::Value::record(em.event_record(), nu::Span::unknown());
    let Ok(event_nuon) = render_nuon(&event) else {
        return;
    };
    let id = mint_msg_id(channel.nonce_gen(), FROM_MCP, em.model(), &event_nuon, None);
    if let Ok(line) = render_packet(id, FROM_MCP, em.model(), &event, None) {
        let _ = channel.emit(line);
    }
}

/// Spawn the sole EmergencyResponder: drain the channel, APPEND each Emergency as a
/// NUON record line to `emergency.nuonl`, and ANNOUNCE it on the packet channel.
///
/// The log is written FIRST and unconditionally, so the durable record never depends
/// on a channel being open. Ends when every producer drops the sender.
pub(crate) fn spawn_emergency_responder(
    mut rx: EmergencyRx,
    mcp_nom: String,
) {
    let path = emergency_log_path(&mcp_nom);
    tk::spawn(async move {
        while let Some(em) = rx.recv().await {
            match em.to_nuon_line(now_millis(), &mcp_nom) {
                Ok(line) => {
                    if let Err(e) = append_line(&path, &line) {
                        eprintln!(
                            "grammar: emergency log append failed at {}: {e}",
                            path.display(),
                        );
                    }
                }
                Err(e) => eprintln!("grammar: emergency serialize failed: {e}"),
            }
            announce(&em);
        }
    });
}
