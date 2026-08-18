//! Cooperative cancellation: kill(nonce) actually stops an in-flight run. A
//! SYSTEM test - it needs a real client sending kill WHILE a run is in flight
//! (pipelined requests over the stdio transport). No test-hooks: nu `sleep` polls
//! Signals, so it bails on the cancel.

use std::time::{Duration, Instant};

use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Poll processes() until an in-flight `run` appears; return its nonce.
fn poll_inflight_run_nonce(host: &mut Host, secs: u64) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let resp = host.call("processes", json!({}));
        if let Some(arr) = structured(&resp).get("processes").and_then(|p| p.as_array()) {
            for e in arr {
                if e.get("tool").and_then(|tl| tl.as_str()) == Some("run") {
                    if let Some(n) = e.get("nonce").and_then(|n| n.as_str()) {
                        return Some(n.to_string());
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Poll processes() until the in-flight set drains (or `secs` elapse).
fn poll_until_drained(host: &mut Host, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let resp = host.call("processes", json!({}));
        if let Some(arr) = structured(&resp).get("processes").and_then(|p| p.as_array()) {
            if arr.is_empty() {
                return true;
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[tested]
fn kill_stops_an_in_flight_run() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    // Launch a long run without blocking on its response.
    let run_id = host.request(
        "run",
        json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"out": "int"},
            "args": {"noop": 0},
            "body": "sleep 30sec\n{ out: 0 }",
        }),
    );
    // It should register as in-flight; grab its nonce.
    let nonce = poll_inflight_run_nonce(&mut host, 5).expect("the run should register as in-flight");
    // Kill it (no-return; its response is ignored).
    let _ = host.request("kill", json!({"nonce": nonce}));
    // Read the run's own response FIRST (before any more processes() reads, which
    // would discard it): a killed run bails on the cancel and returns an error,
    // not a completed result.
    let resp = host.read_id(run_id);
    assert!(
        has_error_path(&resp),
        "a killed run should return an error, not complete; got {resp}",
    );
    // And the in-flight set drains.
    assert!(
        poll_until_drained(&mut host, 5),
        "processes() should drain after the kill",
    );
}
