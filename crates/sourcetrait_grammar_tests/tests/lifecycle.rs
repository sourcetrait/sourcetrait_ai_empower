//! Lifecycle SYSTEM tests for the conditions a nu body cannot trigger on its own
//! (a caught panic; a hung engine thread). They spawn the real host built with the
//! mcp `test-hooks` feature (__test_panic / __test_hang) and are themselves gated
//! behind this crate's `test-hooks` feature, so plain `cargo test --workspace`
//! (which builds a no-hooks binary) skips them. Run:
//!   cargo build -p sourcetrait_grammar_mcp --features test-hooks
//!   cargo test -p sourcetrait_grammar_tests --features test-hooks -- --test-threads=1
#![cfg(feature = "test-hooks")]

use std::time::{Duration, Instant};

use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Poll the per-process emergency log(s) under the namespace's `log/` dir until one
/// carries a hung-thread record, or `secs` elapse; returns the matching (or the
/// last-seen) content. The `<mcp_nom>` subdir is minted at startup, so glob it.
fn wait_for_hung_emergency(host: &Host, id: &str, ns: &str, secs: u64) -> String {
    let log_root = namespace_dir(host.cache_home(), id, ns).join("log");
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut last = String::new();
    loop {
        if let Ok(entries) = std::fs::read_dir(&log_root) {
            for e in entries.flatten() {
                if let Ok(content) = std::fs::read_to_string(e.path().join("emergency.nuonl")) {
                    last = content;
                    if last.contains("hung_engine_thread") {
                        return last;
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return last;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[test]
#[named]
fn panic_in_run_is_caught_host_survives() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    // __test_panic panics the eval thread; catch_unwind catches it -> the run
    // errors, the host is NOT torn down.
    let env = host.run(json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "__test_panic",
    }));
    assert!(
        has_error_path(&env),
        "a panicking eval should surface as an error, not crash the host; got {env}",
    );
    // The host is alive: a normal run still works.
    let ok = host.run(json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": 5},
        "body": "{ out: ($args.x + 1) }",
    }));
    assert_eq!(
        structured(&ok)["result"]["out"].as_i64(),
        Some(6),
        "the host should survive the caught panic; got {ok}",
    );
}

#[test]
#[named]
fn interact_reset_on_panic_drops_session_keeps_host() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    // Seed persistent session state.
    let a = host.call(
        "interact",
        json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"ok": "bool"},
            "args": {"noop": 0},
            "body": "$env.KEEP = \"before\"\n{ ok: true }",
        }),
    );
    assert!(!has_error_path(&a), "seed interact should succeed; got {a}");
    // Panic the interact eval -> caught, the lane rebuilds its engine (session
    // lost), the host survives.
    let p = host.call(
        "interact",
        json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"ok": "bool"},
            "args": {"noop": 0},
            "body": "__test_panic",
        }),
    );
    assert!(has_error_path(&p), "a panicking interact eval should surface as an error; got {p}");
    // Host + lane survive, and the pre-panic session state is GONE (reset).
    let b = host.call(
        "interact",
        json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"keep": "string"},
            "args": {"noop": 0},
            "body": "{ keep: ($env.KEEP? | default \"gone\") }",
        }),
    );
    assert!(!has_error_path(&b), "interact should work after the reset; got {b}");
    assert_eq!(
        structured(&b)["result"]["keep"].as_str(),
        Some("gone"),
        "the reset should have dropped the pre-panic session state; got {b}",
    );
}

#[test]
#[named]
fn hung_thread_is_confirmed_in_emergency_log() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "hid"]);
    // __test_hang blocks uninterruptibly; a short timeout triggers cancel, but the
    // bare sleep never polls Signals, so the thread outlives its cancel and the
    // watchdog confirms it a hung engine thread (past the 5s grace) and logs it.
    let timed = host.call(
        "run",
        json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"out": "int"},
            "args": {"noop": 0},
            "body": "__test_hang",
            "timeout_ms": 500,
        }),
    );
    assert_eq!(envelope_error_kind(&timed), Some("thread::timeout"), "got {timed}");
    let nonce = envelope_error(&timed)
        .and_then(|e| e.get("nonce"))
        .and_then(|n| n.as_str())
        .expect("timeout envelope carries a nonce")
        .to_string();

    // The watchdog samples every 2s and confirms past a 5s grace; wait it out.
    let log = wait_for_hung_emergency(&host, "hid", "default", 15);
    let hit = log
        .lines()
        .find(|l| l.contains("hung_engine_thread") && l.contains(&nonce));
    assert!(
        hit.is_some(),
        "expected a hung_engine_thread emergency for nonce {nonce}; log=\n{log}",
    );
    assert!(
        hit.unwrap().contains("stateless"),
        "the hang should be lane=stateless; got {}",
        hit.unwrap(),
    );
}
