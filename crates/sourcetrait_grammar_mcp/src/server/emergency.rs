//! The emergency lane: classify resource trouble, log it, announce it.
use crate::*;

/// Producer end of the EmergencyChannel; unbounded so a producer never blocks.
pub(crate) type EmergencyTx = tk::UnboundedSender<Emergency>;
/// Consumer end, drained by the sole responder.
pub(crate) type EmergencyRx = tk::UnboundedReceiver<Emergency>;

/// Copy discriminant mirror of `Emergency`, without the payload.
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

/// A cancelled engine thread still alive past the grace window.
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

/// Sustained host-process CPU over the sample window.
#[derive(Clone, Debug)]
pub(crate) struct CpuWarningEmergency {
    pub cpu_pct: f64,
    pub sample_ms: u64,
}

/// Host-process resident set size above the threshold.
#[derive(Clone, Debug)]
pub(crate) struct RamWarningEmergency {
    pub rss_kb: u64,
}

/// GPU memory in use above the line, summed across cards.
#[derive(Clone, Debug)]
pub(crate) struct VramWarningEmergency {
    pub used_mib: u64,
    pub total_mib: u64,
}

/// The environment-wide jobs table above the threshold count.
#[derive(Clone, Debug)]
pub(crate) struct BackgroundJobsWarningEmergency {
    pub job_count: usize,
}

/// A watched filesystem reached the warning line.
#[derive(Clone, Debug)]
pub(crate) struct DiskWarningEmergency {
    pub mount: String,
    pub used_pct: u32,
    pub threshold_pct: u32,
}

/// An origin crossed the SOFT send threshold, once per origin.
#[derive(Clone, Debug)]
pub(crate) struct ChannelSpamWarningEmergency {
    pub origin: String,
    pub hits: u32,
    pub window_secs: u64,
    pub rate: u32,
}

/// An origin crossed the HARD threshold and was stopped.
#[derive(Clone, Debug)]
pub(crate) struct ChannelSpamErrorEmergency {
    pub origin: String,
    pub hits: u32,
    pub window_secs: u64,
    pub rate: u32,
    pub action: String,
}

/// The unambiguous total-failure case; the response is an MCP restart.
#[derive(Clone, Debug)]
pub(crate) struct CriticalEmergency {
    pub reason: String,
    pub hung: usize,
    pub cap: usize,
}

/// A classified resource condition, produced by the watchdog.
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
                r.insert("origin", nu::Value::string(w.origin.clone(), span));
                r.insert("hits", nu::Value::int(w.hits as i64, span));
                r.insert("window_secs", nu::Value::int(w.window_secs as i64, span));
                r.insert("rate", nu::Value::int(w.rate as i64, span));
            }
            Self::ChannelSpamError(e) => {
                r.insert("origin", nu::Value::string(e.origin.clone(), span));
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

    /// Render this Emergency as a SINGLE-LINE NUON record.
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

/// The per-process emergency log path.
pub(crate) fn emergency_log_path(mcp_nom: &str) -> PathBuf {
    cache_base_dir().join("log").join(mcp_nom).join("emergency.nuonl")
}

/// Append one already-rendered NUON line, creating the parent dir on demand.
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

/// Put one Emergency on the channel under its reserved `mcp/` model.
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

/// Spawn the sole responder: log each Emergency, then announce it.
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
