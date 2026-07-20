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
        Some("nushell_mcp eval".to_string()),
        sender,
    )
}

/// Make the host the subreaper so an eval's orphaned grandchildren reparent to
/// us (kept on the /proc ppid chain for the descendant walk). Best-effort; call
/// once at host startup.
#[cfg(target_os = "linux")]
pub(crate) fn install_child_subreaper() {
    if let Err(e) = nix::sys::prctl::set_child_subreaper(true) {
        eprintln!("nushell_mcp: PR_SET_CHILD_SUBREAPER failed: {e}");
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
