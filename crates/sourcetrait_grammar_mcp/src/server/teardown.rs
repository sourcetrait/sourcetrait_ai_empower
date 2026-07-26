use crate::*;

// Layered teardown + reap of an eval's external process tree (EmbedEngine
// principle 2). nushell runs externals headless (is_interactive=false -> plain
// command.spawn, no setpgid), so there is no process group to killpg and a
// ThreadJob tracks only the DIRECT child pid; a grandchild (the child spawns its
// own child) is untracked and a naive SIGKILL of the direct child orphans it.
// The reap that works (P0.3, verified against nushell rev 0df4ca2):
// PR_SET_CHILD_SUBREAPER on the host at startup (orphans reparent to us, staying
// on the /proc ppid chain) + a /proc descendant walk from each tracked child
// BEFORE the kill (deterministic) + SIGKILL each. All Linux-gated; a non-target
// build degrades to a direct-child kill (no /proc walk), keeping the shipped
// crate honest by construction.

/// Build this eval's external-child tracker. Setting it as an engine's
/// `current_job.background_thread_job` makes nushell register the pid of every
/// external the eval spawns into its Arc-shared pid set, which the kill / timeout
/// path reads via `collect_pids()` to tree-kill. The Mail sender is a throwaway:
/// pid tracking (try_add_pid / collect_pids) is sender-independent, and a
/// foreground eval never emits job Mail. Its `Signals` share the eval's cancel
/// flag, so once cancel fires no further pid is registered.
pub(crate) fn make_tracker(cancel: Arc<AtomicBool>) -> nu::ThreadJob {
    let (sender, _rx) = std::sync::mpsc::channel::<nu::Mail>();
    nu::ThreadJob::new(
        nu::Signals::new(cancel),
        Some("grammar eval".to_string()),
        sender,
    )
}

/// How long a zombie must persist before we treat it as ABANDONED and harvest it.
///
/// This grace IS the safety mechanism. A legitimate waiter - `Command::output()` inside
/// `run_git`, or nushell's own external handling - reaps its child within milliseconds
/// of that child exiting, so a zombie still sitting there seconds later belongs to
/// nobody. That is what lets us avoid a blanket `waitpid(-1, WNOHANG)`, which would race
/// those waiters and steal the exit status out from under them; their wait then fails
/// ECHILD, turning a working `run_git` into a spurious error.
const REAP_GRACE_SECS: u64 = 5;

/// Parse `(state, ppid)` out of a `/proc/<pid>/stat` line.
///
/// Fields are counted after the LAST ')', because the comm field is unquoted and can
/// itself contain spaces and parens. After it, index 0 is state (field 3) and index 1 is
/// ppid (field 4).
pub(crate) fn parse_state_ppid(stat: &str) -> Option<(String, u32)> {
    let rparen = stat.rfind(')')?;
    let mut fields = stat[rparen + 1..].split_whitespace();
    let state = fields.next()?.to_string();
    let ppid = fields.next()?.parse().ok()?;
    Some((state, ppid))
}

/// Parse a process's START TIME (field 22 of `/proc/<pid>/stat`, in clock ticks
/// since boot) out of a stat line.
///
/// The field is counted after the LAST ')' for the same reason `parse_state_ppid`
/// does it: the comm field is unquoted and may itself contain spaces and parens.
/// After it, index 0 is state (field 3), so field 22 is index 19.
///
/// ## DEV
/// This exists for the pid-reuse guard on config pins (server/pin.rs). A pid alone
/// cannot say whether the process holding a pin is still the one that took it -
/// pids are recycled, and a long-lived host will outlive plenty of them. The start
/// time makes the pair (pid, start_time) an identity: the kernel will not hand out
/// the same pid with the same start tick, so a pin whose stored start time no
/// longer matches belongs to a process that has already gone.
/// ##
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

/// Off Linux there is no /proc to read, so a pin can never be proven live and the
/// pin surface refuses rather than guessing. We target Linux; this keeps the
/// shipped crate honest by construction rather than by assumption.
#[cfg(not(target_os = "linux"))]
pub(crate) fn process_start_time(_pid: u32) -> Option<u64> {
    None
}

/// Harvests children the subreaper ADOPTED that nobody is waiting on.
///
/// `install_child_subreaper` makes us the parent of every orphan in an eval's process
/// tree - which is what keeps them on our /proc ppid chain for `tree_kill` - and with
/// that we inherit the DUTY to reap them. An adopted child that exits with no waiter
/// otherwise stays a zombie for the life of the host, holding a pid and a process-table
/// slot and polluting every `ps` / /proc sweep we do.
///
/// The recurring source is `git commit`, which detaches its own auto-maintenance
/// (`gc --auto`): one leaked zombie per commit, and the rig lifecycle commits on
/// every new / commit / uninstall. Nothing about it is git-specific though - any
/// external that daemonizes a child lands here the same way.
///
/// Only the SERVE path needs this. A one-shot CLI process exits promptly, at which point
/// its orphans reparent to init and are reaped there.
pub(crate) struct OrphanReaper {
    /// pid -> when we first observed it as a zombie. Entries clear when the pid is
    /// harvested or disappears (someone else got there first).
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

    /// One pass: observe our zombie children, and harvest the ones past the grace
    /// window. Cheap enough for the watchdog's tick - a /proc scan of stat files, and
    /// usually nothing to reap.
    #[cfg(target_os = "linux")]
    pub(crate) fn reap(&mut self) {
        let zombies = zombie_children_of(process::id());
        // Forget anything no longer present, so the map tracks only live zombies.
        self.seen.retain(|pid, _| zombies.contains(pid));
        for pid in zombies {
            let first_seen = *self.seen.entry(pid).or_insert_with(Instant::now);
            if first_seen.elapsed().as_secs() < REAP_GRACE_SECS {
                continue;
            }
            // WNOHANG so a surprise never blocks the watchdog. A pid someone else
            // already reaped returns ECHILD, which is exactly the no-op we want.
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

/// Our own direct children currently in state `Z`. An adopted orphan becomes a direct
/// child of ours on reparent, so this covers both.
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

/// Make the host the subreaper so an eval's orphaned grandchildren reparent to
/// us (kept on the /proc ppid chain for the descendant walk). Best-effort; call
/// once at host startup.
#[cfg(target_os = "linux")]
pub(crate) fn install_child_subreaper() {
    if let Err(e) = nix::sys::prctl::set_child_subreaper(true) {
        eprintln!("grammar: PR_SET_CHILD_SUBREAPER failed: {e}");
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn install_child_subreaper() {}

/// SIGKILL every tracked child AND its /proc descendants (walked first, so a
/// grandchild the tracked child spawned dies too). No-op for an empty slice.
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

/// SIGKILL the host's plugin subprocesses (direct children whose comm starts
/// with `nu_plugin_`). Unblocks a plugin-hung eval: a hung plugin ignores the
/// cancel Signals and is not a tracked external, so killing its subprocess closes
/// the plugin IPC pipe and the eval's plugin read returns an error. Plugins
/// respawn lazily on next use. Broad by nature - nushell shares plugin
/// subprocesses across evals - so it fires only on an explicit kill(nonce) + the
/// shutdown sweep, never on an automatic timeout (the supervisor scopes that
/// escalation, Phase 5).
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

/// Every descendant pid of `root`, read from /proc: parse each `/proc/<pid>/stat`
/// for its ppid (the field after the LAST ')', since the comm field can itself
/// contain spaces / parens), build the ppid map, then BFS out from `root`.
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
        // after the ')': " <state> <ppid> ..."
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
