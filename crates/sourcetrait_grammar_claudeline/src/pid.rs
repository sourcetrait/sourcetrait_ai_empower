use crate::*;

/// What: a live `claude` session process - its pid and working directory - read
/// from `/proc`. Only the two fields the pid match needs are kept.
///
/// Why: claudeline records the pid of the claude process backing this render so
/// the grammar:pid nu helpers can bind a session_nom to a live process (the yaml
/// alone only proves a last write, not that the session still runs).
///
/// Where: produced by read_claude_procs, consumed by select_pid.
pub(crate) struct ClaudeProc {
    pub(crate) pid: i64,
    pub(crate) cwd: String,
}

/// What: the pid of the live claude process whose cwd maps to `identity` (lowest
/// pid when several share it), or None.
///
/// Why: the recorded `pid` lets the grammar:pid helpers match this session to a
/// live process. Best-effort - any `/proc` read failure just yields None and the
/// field is omitted, never failing the render.
///
/// Where: persist_status, injected into the status yaml beside session_nom.
pub(crate) fn detect_pid(identity: &str) -> Option<i64> {
    select_pid(&read_claude_procs(), identity)
}

/// What: the lowest pid among `procs` whose cwd maps to `identity` (via the same
/// `ai_identity` analysis the render uses), or None.
///
/// Why: pure selection split from the `/proc` read so it is unit-testable;
/// lowest pid favors the oldest (main) session when a cwd is shared.
///
/// Where: detect_pid; the unit tests.
pub(crate) fn select_pid(
    procs: &[ClaudeProc],
    identity: &str,
) -> Option<i64> {
    procs
        .iter()
        .filter(|proc| ai_identity(&proc.cwd) == identity)
        .map(|proc| proc.pid)
        .min()
}

/// What: every live claude process as a ClaudeProc row, read from `/proc`.
///
/// Why: a claude session is identified by its command (argv[0] is the claude
/// binary, with arguments) - the same notion nushell `ps`'s `command` column
/// carries, so this agrees with the grammar:pid nu helper's filter. Any
/// unreadable entry is skipped (best-effort).
///
/// Where: detect_pid.
fn read_claude_procs() -> Vec<ClaudeProc> {
    let mut procs = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return procs;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(pid) = name.parse::<i64>() else { continue };
        let dir = entry.path();
        let Ok(cmdline) = fs::read(dir.join("cmdline")) else {
            continue;
        };
        if !is_claude_session(&cmdline) {
            continue;
        }
        let Ok(cwd) = fs::read_link(dir.join("cwd")) else {
            continue;
        };
        let Some(cwd) = cwd.to_str() else { continue };
        procs.push(ClaudeProc {
            pid,
            cwd: cwd.to_string(),
        });
    }
    procs
}

/// The claude session binary; a session's argv[0] equals this exactly.
const CLAUDE_BIN: &[u8] = b"/usr/local/bin/claude";

/// What: true when a `/proc/<pid>/cmdline` is a claude session - argv[0] is
/// exactly the claude binary and at least one argument follows.
///
/// Why: equivalent to nushell `ps`'s `command | str starts-with
/// '/usr/local/bin/claude '` (argv joined by spaces, trailing space requires an
/// argument), so the Rust producer and the nu consumer match the same processes.
///
/// Where: read_claude_procs.
fn is_claude_session(cmdline: &[u8]) -> bool {
    let mut argv = cmdline.split(|byte| *byte == 0).filter(|seg| !seg.is_empty());
    matches!(argv.next(), Some(arg0) if arg0 == CLAUDE_BIN) && argv.next().is_some()
}
