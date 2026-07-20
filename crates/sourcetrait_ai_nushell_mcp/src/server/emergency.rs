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
    HostCpu,
    HostMemory,
    Vram,
    BackgroundJobs,
    Critical,
}

impl EmergencyKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::HungEngineThread => "hung_engine_thread",
            Self::HostCpu => "host_cpu",
            Self::HostMemory => "host_memory",
            Self::Vram => "vram",
            Self::BackgroundJobs => "background_jobs",
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
/// so it is DATA, not an action trigger.
#[derive(Clone, Debug)]
pub(crate) struct HostCpuEmergency {
    pub cpu_pct: f64,
    pub sample_ms: u64,
}

/// Host-process resident set size (VmRSS) above the generous threshold. Data.
#[derive(Clone, Debug)]
pub(crate) struct HostMemoryEmergency {
    pub rss_kb: u64,
}

/// GPU memory in use above the threshold fraction (summed across GPUs, from
/// nvidia-smi). Data - attribution to our eval children is a later refinement.
#[derive(Clone, Debug)]
pub(crate) struct VramEmergency {
    pub used_mib: u64,
    pub total_mib: u64,
}

/// The host-owned environment-wide jobs table (P0.8) above the threshold count.
/// A body's `job spawn` persists past its eval; an unbounded accrual is the
/// signal. Data.
#[derive(Clone, Debug)]
pub(crate) struct BackgroundJobsEmergency {
    pub job_count: usize,
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
    HostCpu(HostCpuEmergency),
    HostMemory(HostMemoryEmergency),
    Vram(VramEmergency),
    BackgroundJobs(BackgroundJobsEmergency),
    Critical(CriticalEmergency),
}

impl Emergency {
    pub(crate) fn kind(&self) -> EmergencyKind {
        match self {
            Self::HungEngineThread(_) => EmergencyKind::HungEngineThread,
            Self::HostCpu(_) => EmergencyKind::HostCpu,
            Self::HostMemory(_) => EmergencyKind::HostMemory,
            Self::Vram(_) => EmergencyKind::Vram,
            Self::BackgroundJobs(_) => EmergencyKind::BackgroundJobs,
            Self::Critical(_) => EmergencyKind::Critical,
        }
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
            Self::HostCpu(c) => {
                r.insert("cpu_pct", nu::Value::float(c.cpu_pct, span));
                r.insert("sample_ms", nu::Value::int(c.sample_ms as i64, span));
            }
            Self::HostMemory(m) => {
                r.insert("rss_kb", nu::Value::int(m.rss_kb as i64, span));
            }
            Self::Vram(v) => {
                r.insert("used_mib", nu::Value::int(v.used_mib as i64, span));
                r.insert("total_mib", nu::Value::int(v.total_mib as i64, span));
            }
            Self::BackgroundJobs(b) => {
                r.insert("job_count", nu::Value::int(b.job_count as i64, span));
            }
            Self::Critical(c) => {
                r.insert("reason", nu::Value::string(c.reason.clone(), span));
                r.insert("hung", nu::Value::int(c.hung as i64, span));
                r.insert("cap", nu::Value::int(c.cap as i64, span));
            }
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

/// Spawn the sole EmergencyResponder: drain the channel and APPEND each Emergency
/// as a NUON record line to `emergency.nuonl`. Logging is its ONLY action
/// (classify-first). Ends when every producer (the watchdog) drops the sender.
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
                            "nushell_mcp: emergency log append failed at {}: {e}",
                            path.display(),
                        );
                    }
                }
                Err(e) => eprintln!("nushell_mcp: emergency serialize failed: {e}"),
            }
        }
    });
}
