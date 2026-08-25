use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_kind, has_error};
use sourcetrait_common::testing::prelude::*;

/// One shared in-process server per test binary: constructing a TestServer runs
/// the namespace substrate (keypair, rigs repo git config), which must not race
/// itself across parallel tests.
static TESTING: testing::ModuleWith<TestServer> = testing::module_with!(Integration, {
    .setup(|_| TestServer::new())
});

#[test]
fn run_envelope_has_nonce_and_no_rerun_id() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 1}),
        "{ out: ($args.x + 100) }",
    );
    assert!(
        env["nonce"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "run envelope should carry a non-empty nonce; got {env}",
    );
    assert!(env.get("rerun_id").is_none(), "run envelope should not carry rerun_id; got {env}");
}

#[test]
fn timeout_then_rerun_recovers() {
    let s = TESTING.harness();
    let timed = s.run_timeout(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 5}),
        "sleep 400ms\n{ out: ($args.x + 1) }",
        Some(100),
    );
    assert_eq!(error_kind(&timed), Some("thread::timeout"), "first run should time out; got {timed}");
    let nonce = timed["error"]["nonce"].as_str().expect("timeout nonce").to_string();

    // The body was cached PRE-dispatch, so the timed-out nonce is a valid rerun
    // handle - replay with a larger timeout recovers it.
    let recovered = s.rerun_timeout(&nonce, json!({"x": 5}), Some(5000));
    assert_eq!(recovered["result"]["out"].as_i64(), Some(6), "rerun of a timed-out nonce should complete; got {recovered}");
}

#[test]
fn rerun_by_nonce_roundtrips_with_new_args() {
    let s = TESTING.harness();
    let first = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 5}),
        "{ out: ($args.x * 3) }",
    );
    assert_eq!(first["result"]["out"].as_i64(), Some(15));
    let nonce = first["nonce"].as_str().expect("run nonce").to_string();

    let second = s.rerun(&nonce, json!({"x": 7}));
    assert_eq!(second["result"]["out"].as_i64(), Some(21), "rerun with new args -> 7 * 3; got {second}");
    assert!(second.get("rerun_id").is_none(), "got {second}");

    // The rerun's OWN nonce is itself a handle (rerun caches its body too).
    let rerun_nonce = second["nonce"].as_str().expect("rerun nonce").to_string();
    assert_ne!(rerun_nonce, nonce, "each eval gets a fresh nonce");
    let third = s.rerun(&rerun_nonce, json!({"x": 2}));
    assert_eq!(third["result"]["out"].as_i64(), Some(6), "a rerun's nonce must itself be rerunnable; got {third}");
}

#[test]
fn rerun_unknown_nonce_errors() {
    let s = TESTING.harness();
    let env = s.rerun("abcDEF123456", json!({"x": 0}));
    assert!(has_error(&env), "a base62 nonce with no cached body should error; got {env}");
}

#[test]
fn rerun_rejects_non_base62_nonce() {
    let s = TESTING.harness();
    let env = s.rerun("../etc/passwd", json!({"x": 0}));
    assert!(has_error(&env), "a non-base62 nonce should error; got {env}");
}

#[test]
fn interact_envelope_has_no_rerun_id() {
    let s = TESTING.harness();
    let env = s.interact(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 4}),
        "{ out: ($args.x * 2) }",
    );
    assert_eq!(env["result"]["out"].as_i64(), Some(8));
    assert!(env.get("rerun_id").is_none(), "interact envelope should not carry rerun_id; got {env}");
}
