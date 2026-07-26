//! The watchdog: sample the host, classify trouble, emit it edge-triggered.
use crate::*;

/// Which eval substrate a hung engine thread belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    Stateless,
    Interact,
}

impl Lane {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stateless => "stateless",
            Self::Interact => "interact",
        }
    }
}

/// A cancelled engine thread the watchdog watches for a hang.
#[derive(Clone)]
pub(crate) struct HungWatch {
    pub nonce: String,
    pub tool: &'static str,
    pub lane: Lane,
    pub started_at: u64,
    pub cancelled_at: u64,
    pub finished: Arc<AtomicBool>,
}

/// The cancelled-thread registry the watchdog scans for hangs. Keyed by nonce.
pub(crate) type HungRegistry = Arc<std::sync::Mutex<HashMap<String, HungWatch>>>;

/// Record a cancelled-but-maybe-alive engine thread for the watchdog.
pub(crate) fn register_hung(
    registry: &HungRegistry,
    watch: HungWatch,
) {
    let mut map = registry.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(watch.nonce.clone()).or_insert(watch);
}

/// How often the watchdog samples.
const SAMPLE_INTERVAL: tk::TkDuration = tk::TkDuration::from_secs(2);
/// A cancelled thread still alive this long past its cancel is hung.
const HUNG_GRACE_MS: u64 = 5_000;

/// How long a level must HOLD before it is worth telling the agent about.
const LEVEL_SUSTAIN: tk::TkDuration = tk::TkDuration::from_secs(60);
/// The floor between two warnings about the SAME condition.
const LEVEL_REWARN: tk::TkDuration = tk::TkDuration::from_secs(30 * 60);

/// Environment-wide background job count.
const JOBS_THRESHOLD: f64 = 32.0;
/// nvidia-smi is sampled every Nth tick (it is a subprocess; keep it coarse).
const VRAM_SAMPLE_EVERY: u64 = 8;

/// Total system RAM in kB, read once from /proc/meminfo.
#[cfg(target_os = "linux")]
fn total_ram_kb() -> Option<f64> {
    static TOTAL: OnceLock<Option<u64>> = OnceLock::new();
    let cached = TOTAL.get_or_init(|| {
        let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
        parse_mem_total_kb(&meminfo)
    });
    (*cached).map(|kb| kb as f64)
}

#[cfg(not(target_os = "linux"))]
fn total_ram_kb() -> Option<f64> {
    None
}

/// Parse `MemTotal:` (kB) out of /proc/meminfo contents.
pub(crate) fn parse_mem_total_kb(meminfo: &str) -> Option<u64> {
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

/// `df` is SLOW, so it is sampled rarely.
const DISK_SAMPLE_EVERY: u64 = 150;

/// Parse `df -P` output into `(mount, used_pct)` rows.
pub(crate) fn parse_df(output: &str) -> Vec<(String, u32)> {
    let mut rows = Vec::new();
    for line in output.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let Ok(used_pct) = fields[4].trim_end_matches('%').parse::<u32>() else {
            continue;
        };
        rows.push((fields[5..].join(" "), used_pct));
    }
    rows
}

/// Which filesystems we watch, and their gates.
#[derive(Default)]
pub(crate) struct DiskWatch {
    gates: HashMap<String, LevelGate>,
    baselined: bool,
}

impl DiskWatch {
    pub(crate) fn sample(
        &mut self,
        rows: &[(String, u32)],
        threshold_pct: u32,
        now: Instant,
    ) -> Vec<Emergency> {
        if !self.baselined {
            self.baselined = true;
            for (mount, used_pct) in rows {
                if *used_pct < threshold_pct {
                    self.gates.insert(mount.clone(), LevelGate::default());
                }
            }
            return Vec::new();
        }
        let mut out = Vec::new();
        for (mount, used_pct) in rows {
            if let Some(gate) = self.gates.get_mut(mount)
                && gate.sample(
                    *used_pct as f64,
                    threshold_pct as f64,
                    now,
                    LEVEL_SUSTAIN,
                    LEVEL_REWARN,
                )
            {
                out.push(Emergency::DiskWarning(DiskWarningEmergency {
                    mount: mount.clone(),
                    used_pct: *used_pct,
                    threshold_pct,
                }));
            }
        }
        out
    }
}

/// Run `df -P` on a blocking pool thread.
async fn sample_disk() -> Option<Vec<(String, u32)>> {
    tk::spawn_blocking(|| {
        let out = std::process::Command::new("df").arg("-P").output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(parse_df(&String::from_utf8_lossy(&out.stdout)))
    })
    .await
    .ok()
    .flatten()
}

/// The CPU line, as a percentage: all cores busy is `cores * 100`.
fn cpu_capacity_pct() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64 * 100.0)
        .unwrap_or(100.0)
}

/// A debounced level detector with a long re-warn floor.
#[derive(Default)]
pub(crate) struct LevelGate {
    over_since: Option<Instant>,
    last_fired: Option<Instant>,
}

impl LevelGate {
    pub(crate) fn sample(
        &mut self,
        value: f64,
        fire_at: f64,
        now: Instant,
        sustain: tk::TkDuration,
        rewarn: tk::TkDuration,
    ) -> bool {
        if value < fire_at {
            self.over_since = None;
            return false;
        }
        let since = *self.over_since.get_or_insert(now);
        if now.saturating_duration_since(since) < sustain {
            return false;
        }
        if let Some(last) = self.last_fired
            && now.saturating_duration_since(last) < rewarn
        {
            return false;
        }
        self.last_fired = Some(now);
        true
    }
}

/// Linux `_SC_CLK_TCK`; the CPU conversion assumes the standard 100.
const CLK_TCK: f64 = 100.0;

/// May the watchdog SAMPLE on this tick?
fn sampling_online() -> bool {
    !config().test || !matches!(channel_handle().status().phase, ChannelPhase::Closed)
}

/// Everything the watchdog samples. All Arc handles onto NuSh state.
pub(crate) struct WatchdogDeps {
    pub hung_watch: HungRegistry,
    pub semaphore: Arc<tk::Semaphore>,
    pub cap: usize,
    pub env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
    pub tx: EmergencyTx,
}

/// Prune finished entries and confirm the rest as hung past `grace_ms`.
pub(crate) fn scan_hung(
    registry: &HungRegistry,
    now: u64,
    grace_ms: u64,
    pool_held: usize,
    pool_cap: usize,
) -> (Vec<(String, Emergency)>, usize) {
    let mut out = Vec::new();
    let mut confirmed_stateless = 0usize;
    let mut map = registry.lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, w| !w.finished.load(Ordering::SeqCst));
    for w in map.values() {
        let hung_ms = now.saturating_sub(w.cancelled_at);
        if hung_ms <= grace_ms {
            continue;
        }
        if matches!(w.lane, Lane::Stateless) {
            confirmed_stateless += 1;
        }
        out.push((
            format!("hung:{}:{}", w.lane.as_str(), w.nonce),
            Emergency::HungEngineThread(HungEngineThreadEmergency {
                nonce: w.nonce.clone(),
                lane: w.lane.as_str().to_string(),
                tool: w.tool.to_string(),
                started_at: w.started_at,
                cancelled_at: w.cancelled_at,
                hung_ms,
                pool_held,
                pool_cap,
            }),
        ));
    }
    (out, confirmed_stateless)
}

/// Parse VmRSS (kB) out of `/proc/self/status` contents.
pub(crate) fn parse_rss_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

/// Parse cumulative CPU jiffies (utime + stime) out of a stat line.
pub(crate) fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let rparen = stat.rfind(')')?;
    let fields: Vec<&str> = stat[rparen + 1..].split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Sum `(used_mib, total_mib)` across the nvidia-smi CSV rows.
pub(crate) fn parse_vram(csv: &str) -> Option<(u64, u64)> {
    let mut used_sum = 0u64;
    let mut total_sum = 0u64;
    for line in csv.lines() {
        let mut parts = line.split(',');
        let (Some(u), Some(t)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(u), Ok(t)) = (u.trim().parse::<u64>(), t.trim().parse::<u64>()) else {
            continue;
        };
        used_sum += u;
        total_sum += t;
    }
    if total_sum == 0 {
        None
    } else {
        Some((used_sum, total_sum))
    }
}

#[cfg(target_os = "linux")]
fn read_rss_kb() -> Option<u64> {
    parse_rss_kb(&fs::read_to_string("/proc/self/status").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn read_rss_kb() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn read_cpu_ticks() -> Option<u64> {
    parse_cpu_ticks(&fs::read_to_string("/proc/self/stat").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn read_cpu_ticks() -> Option<u64> {
    None
}

/// Runtime-probe nvidia-smi for summed GPU memory.
async fn sample_vram() -> Option<(u64, u64)> {
    tk::spawn_blocking(|| {
        let out = std::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_vram(&String::from_utf8_lossy(&out.stdout))
    })
    .await
    .ok()
    .flatten()
}

/// Count the host-owned environment-wide background jobs. Poison-tolerant.
fn job_count(env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>) -> usize {
    let jobs = env_jobs.lock().unwrap_or_else(|e| e.into_inner());
    jobs.iter().count()
}

/// Spawn the watchdog task.
pub(crate) fn spawn_watchdog(deps: WatchdogDeps) {
    tk::spawn(async move {
        let mut prev_active: HashSet<String> = HashSet::new();
        let mut prev_cpu: Option<(u64, Instant)> = None;
        let mut tick: u64 = 0;
        let mut reaper = OrphanReaper::new();
        let mut cpu_gate = LevelGate::default();
        let mut ram_gate = LevelGate::default();
        let mut vram_gate = LevelGate::default();
        let mut jobs_gate = LevelGate::default();
        let mut disk = DiskWatch::default();
        loop {
            tk::sleep(SAMPLE_INTERVAL).await;
            tick += 1;
            reaper.reap();
            reap_pins();
            if !sampling_online() {
                prev_cpu = None;
                prev_active.clear();
                cpu_gate = LevelGate::default();
                ram_gate = LevelGate::default();
                vram_gate = LevelGate::default();
                jobs_gate = LevelGate::default();
                disk = DiskWatch::default();
                continue;
            }
            let supervisor = effective_supervisor();
            let now = now_millis();
            let mut current: Vec<(String, Emergency)> = Vec::new();

            let held = deps.cap.saturating_sub(deps.semaphore.available_permits());
            let (hung, confirmed_stateless) =
                scan_hung(&deps.hung_watch, now, HUNG_GRACE_MS, held, deps.cap);
            current.extend(hung);
            if deps.cap > 0 && confirmed_stateless >= deps.cap {
                current.push((
                    "critical:stateless_saturation".to_string(),
                    Emergency::Critical(CriticalEmergency {
                        reason: "all stateless engine-thread permits held by hung threads; run() cannot dispatch"
                            .to_string(),
                        hung: confirmed_stateless,
                        cap: deps.cap,
                    }),
                ));
            }

            let sample_at = Instant::now();
            let jobs = job_count(&deps.env_jobs);
            if jobs_gate.sample(
                jobs as f64,
                JOBS_THRESHOLD,
                sample_at,
                LEVEL_SUSTAIN,
                LEVEL_REWARN,
            ) {
                let _ = deps.tx.send(Emergency::BackgroundJobsWarning(
                    BackgroundJobsWarningEmergency { job_count: jobs },
                ));
            }

            if let Some(rss) = read_rss_kb()
                && let Some(total_kb) = total_ram_kb()
                && ram_gate.sample(
                    rss as f64,
                    total_kb * supervisor.ram_warn_fraction,
                    sample_at,
                    LEVEL_SUSTAIN,
                    LEVEL_REWARN,
                )
            {
                let _ = deps
                    .tx
                    .send(Emergency::RamWarning(RamWarningEmergency { rss_kb: rss }));
            }

            let sample_now = Instant::now();
            let cur_ticks = read_cpu_ticks();
            if let (Some(ct), Some((pt, pinst))) = (cur_ticks, prev_cpu) {
                let elapsed = sample_now.duration_since(pinst).as_secs_f64();
                if elapsed > 0.0 {
                    let cpu_pct = (ct.saturating_sub(pt) as f64 / CLK_TCK) / elapsed * 100.0;
                    if cpu_gate.sample(
                        cpu_pct,
                        cpu_capacity_pct() * supervisor.cpu_warn_fraction,
                        sample_at,
                        LEVEL_SUSTAIN,
                        LEVEL_REWARN,
                    ) {
                        let _ = deps.tx.send(Emergency::CpuWarning(CpuWarningEmergency {
                            cpu_pct,
                            sample_ms: (elapsed * 1000.0) as u64,
                        }));
                    }
                }
            }
            if let Some(ct) = cur_ticks {
                prev_cpu = Some((ct, sample_now));
            }

            if tick % VRAM_SAMPLE_EVERY == 0
                && let Some((used, total)) = sample_vram().await
                && total > 0
                && vram_gate.sample(
                    used as f64,
                    (total as f64 - supervisor.vram_warn_headroom_mib as f64).max(0.0),
                    Instant::now(),
                    LEVEL_SUSTAIN,
                    LEVEL_REWARN,
                )
            {
                let _ = deps.tx.send(Emergency::VramWarning(VramWarningEmergency {
                    used_mib: used,
                    total_mib: total,
                }));
            }

            if (tick == 1 || tick % DISK_SAMPLE_EVERY == 0)
                && let Some(rows) = sample_disk().await
            {
                let threshold_pct = (supervisor.disk_warn_fraction * 100.0) as u32;
                for emergency in disk.sample(&rows, threshold_pct, Instant::now()) {
                    let _ = deps.tx.send(emergency);
                }
            }

            let keys: HashSet<String> = current.iter().map(|(k, _)| k.clone()).collect();
            for (k, em) in current {
                if !prev_active.contains(&k) {
                    let _ = deps.tx.send(em);
                }
            }
            prev_active = keys;
        }
    });
}
