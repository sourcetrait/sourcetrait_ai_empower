//! Layered teardown and reap of an eval's external process tree.
use crate::*;

/// Build this eval's external-child tracker.
pub(crate) fn make_tracker(cancel: Arc<AtomicBool>) -> nu::ThreadJob {
    let (sender, _rx) = std::sync::mpsc::channel::<nu::Mail>();
    nu::ThreadJob::new(
        nu::Signals::new(cancel),
        Some("grammar eval".to_string()),
        sender,
    )
}

/// How long a zombie must persist before we treat it as ABANDONED.
const REAP_GRACE_SECS: u64 = 5;

/// Parse `(state, ppid)` out of a `/proc/<pid>/stat` line.
pub(crate) fn parse_state_ppid(stat: &str) -> Option<(String, u32)> {
    let rparen = stat.rfind(')')?;
    let mut fields = stat[rparen + 1..].split_whitespace();
    let state = fields.next()?.to_string();
    let ppid = fields.next()?.parse().ok()?;
    Some((state, ppid))
}

/// Parse a process's START TIME (field 22) out of a stat line.
pub(crate) fn parse_start_time(stat: &str) -> Option<u64> {
    let rparen = stat.rfind(')')?;
    stat[rparen + 1..]
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

/// A live process's start time, or None when the pid is not running.
#[cfg(target_os = "linux")]
pub(crate) fn process_start_time(pid: u32) -> Option<u64> {
    parse_start_time(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// Off Linux there is no /proc, so a pin can never be proven live.
#[cfg(not(target_os = "linux"))]
pub(crate) fn process_start_time(_pid: u32) -> Option<u64> {
    None
}

/// Harvests children the subreaper ADOPTED that nobody is waiting on.
pub(crate) struct OrphanReaper {
    seen: HashMap<u32, Instant>,
}

impl Default for OrphanReaper {
    fn default() -> Self {
        Self::new()
    }
}

impl OrphanReaper {
    pub(crate) fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// One pass: observe our zombies, harvest the ones past the grace window.
    #[cfg(target_os = "linux")]
    pub(crate) fn reap(&mut self) {
        let zombies = zombie_children_of(process::id());
        self.seen.retain(|pid, _| zombies.contains(pid));
        for pid in zombies {
            let first_seen = *self.seen.entry(pid).or_insert_with(Instant::now);
            if first_seen.elapsed().as_secs() < REAP_GRACE_SECS {
                continue;
            }
            let _ = nix::sys::wait::waitpid(
                nix::unistd::Pid::from_raw(pid as i32),
                Some(nix::sys::wait::WaitPidFlag::WNOHANG),
            );
            self.seen.remove(&pid);
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn reap(&mut self) {}
}

/// Our own direct children currently in state `Z`.
#[cfg(target_os = "linux")]
fn zombie_children_of(parent: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        if let Some((state, ppid)) = parse_state_ppid(&stat)
            && state == "Z"
            && ppid == parent
        {
            out.push(pid);
        }
    }
    out
}

/// Make the host the subreaper so an eval's orphans stay reachable.
#[cfg(target_os = "linux")]
pub(crate) fn install_child_subreaper() {
    if let Err(e) = nix::sys::prctl::set_child_subreaper(true) {
        eprintln!("grammar: PR_SET_CHILD_SUBREAPER failed: {e}");
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn install_child_subreaper() {}

/// SIGKILL every tracked child AND its /proc descendants.
#[cfg(target_os = "linux")]
pub(crate) fn tree_kill(tracked: &[u32]) {
    let mut victims: Vec<u32> = Vec::new();
    for &root in tracked {
        for descendant in descendants_of(root) {
            if !victims.contains(&descendant) {
                victims.push(descendant);
            }
        }
        if !victims.contains(&root) {
            victims.push(root);
        }
    }
    for pid in victims {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn tree_kill(tracked: &[u32]) {
    for &pid in tracked {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// SIGKILL the host's plugin subprocesses.
#[cfg(target_os = "linux")]
pub(crate) fn kill_plugin_subprocesses() {
    let me = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let (Some(lparen), Some(rparen)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let comm = &stat[lparen + 1..rparen];
        let mut fields = stat[rparen + 1..].split_whitespace();
        let _state = fields.next();
        let Some(ppid) = fields.next().and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if ppid == me && comm.starts_with("nu_plugin_") {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn kill_plugin_subprocesses() {}

/// Every descendant pid of `root`, read from /proc.
#[cfg(target_os = "linux")]
fn descendants_of(root: u32) -> Vec<u32> {
    let mut ppid_of: HashMap<u32, u32> = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(rparen) = stat.rfind(')') else {
            continue;
        };
        let mut fields = stat[rparen + 1..].split_whitespace();
        let _state = fields.next();
        if let Some(ppid) = fields.next().and_then(|p| p.parse::<u32>().ok()) {
            ppid_of.insert(pid, ppid);
        }
    }
    let mut out: Vec<u32> = Vec::new();
    let mut stack = vec![root];
    while let Some(parent) = stack.pop() {
        for (&pid, &ppid) in &ppid_of {
            if ppid == parent && pid != root && !out.contains(&pid) {
                out.push(pid);
                stack.push(pid);
            }
        }
    }
    out
}
