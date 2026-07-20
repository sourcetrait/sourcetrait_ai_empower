//! External teardown: a timed-out eval's external child is reaped (the Phase 2
//! timeout -> cancel -> tree-kill path). In-process: `run_timeout` drives the real
//! dispatch, and the reap is asserted by scanning /proc for the child. Linux-only
//! (the reap + this scan are /proc-based; the crate targets Linux).

use std::time::{Duration, Instant};

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_kind};

/// Any `/proc/<pid>/cmdline` containing `marker` (the child is distinctively
/// argged so this finds ours, not an unrelated process).
fn any_cmdline_contains(marker: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for e in entries.flatten() {
        if let Ok(bytes) = std::fs::read(e.path().join("cmdline")) {
            if String::from_utf8_lossy(&bytes).contains(marker) {
                return true;
            }
        }
    }
    false
}

/// SIGKILL is asynchronous, so poll for the child to disappear.
fn wait_until_gone(marker: &str, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if !any_cmdline_contains(marker) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn timed_out_external_is_reaped() {
    let s = TestServer::new();
    // The eval blocks on `^sleep 31337`; the 500ms timeout triggers cancel + a
    // tree-kill of the tracked external, which also unblocks the waiting thread.
    let env = s.run_timeout(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "^sleep 31337\n{ out: 0 }",
        Some(500),
    );
    assert_eq!(error_kind(&env), Some("thread::timeout"), "got {env}");
    assert!(
        wait_until_gone("31337", 10),
        "the timed-out `sleep 31337` external should be reaped by the tree-kill",
    );
}
