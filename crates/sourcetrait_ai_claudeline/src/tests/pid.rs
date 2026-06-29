use crate::pid::{
    ClaudeProc,
    select_pid,
};

fn proc(
    pid: i64,
    cwd: &str,
) -> ClaudeProc {
    ClaudeProc {
        pid,
        cwd: cwd.to_string(),
    }
}

#[test]
fn matches_basename_identity() {
    let procs = vec![proc(100, "/home/box/ai/emptwo")];
    assert_eq!(select_pid(&procs, "emptwo"), Some(100));
}

#[test]
fn maps_colony_worktree_to_ant_identity() {
    let procs = vec![proc(200, "/home/box/ai/ant/colony/emptwo")];
    assert_eq!(select_pid(&procs, "ant_emptwo"), Some(200));
    assert_eq!(select_pid(&procs, "emptwo"), None);
}

#[test]
fn returns_lowest_pid_when_cwd_shared() {
    let procs = vec![
        proc(300, "/home/box/ai/emptwo"),
        proc(150, "/home/box/ai/emptwo"),
    ];
    assert_eq!(select_pid(&procs, "emptwo"), Some(150));
}

#[test]
fn none_when_no_cwd_matches() {
    let procs = vec![proc(100, "/home/box/ai/other")];
    assert_eq!(select_pid(&procs, "emptwo"), None);
}
