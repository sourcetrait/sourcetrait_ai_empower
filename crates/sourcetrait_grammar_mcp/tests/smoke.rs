//! Domain smoke tests, driven in-process via `guts::TestServer`. The raw-stdio
//! transport (the JSON-RPC handshake + tools/list framing) is a SYSTEM concern
//! covered by `sourcetrait_grammar_tests` (host_run_tool); these exercise what
//! run / processes / kill actually DO.

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_kind, error_text, has_error};
use sourcetrait_common::testing::prelude::*;

/// One shared in-process server per test binary: constructing a TestServer runs
/// the namespace substrate (keypair, rigs repo git config), which must not race
/// itself across parallel tests.
static TESTING: testing::ModuleWith<TestServer> = testing::module_with!(Integration, {
    .setup(|_| TestServer::new())
});

#[test]
fn arg_typecheck_error_surfaces() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": "five"}),
        "{ out: ($args.x + 1) }",
    );
    assert!(has_error(&env), "a parse-time arg mismatch should surface as error; got {env}");
}

#[test]
fn result_typecheck_error_surfaces() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 5}),
        "{ out: \"five\" }",
    );
    assert!(has_error(&env), "a runtime result mismatch should surface as error; got {env}");
}

#[test]
fn external_command_runs() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "string"}),
        json!({"noop": 0}),
        "{ out: (^printf hello | str trim) }",
    );
    assert_eq!(env["result"]["out"].as_str(), Some("hello"), "got {env}");
}

#[test]
fn exit_decl_is_unreachable() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ out: (exit 1; 0) }",
    );
    assert!(
        has_error(&env),
        "`exit` should be a disabled decl, not a host-fatal process exit; got {env}"
    );
    assert!(
        error_text(&env).contains("disabled"),
        "`exit` must be shadowed by an erroring decl in-process (no host-fatal exit); got {env}",
    );
}

#[test]
fn timeout_fires_then_recovers() {
    let s = TESTING.harness();
    let timed = s.run_timeout(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "sleep 5sec\n{ out: 0 }",
        Some(200),
    );
    assert_eq!(error_kind(&timed), Some("thread::timeout"), "got {timed}");
    assert!(
        timed["error"]["errors"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("200"),
        "timeout message should carry the ms; got {timed}",
    );
    assert!(timed["error"]["nonce"].as_str().is_some(), "expected nonce; got {timed}");

    let env = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 7}),
        "{ out: ($args.x + 1) }",
    );
    assert_eq!(env["result"]["out"].as_i64(), Some(8), "a fresh run after a timeout should work; got {env}");
}

#[test]
fn processes_empty_when_idle() {
    // A PRIVATE server: on the shared harness a sibling test's in-flight eval
    // would legitimately show here. Touch the harness first so this private
    // construction never races the module's one-time substrate init.
    let _ = TESTING.harness();
    let s = TestServer::new();
    let env = s.processes();
    assert!(
        env["processes"].as_array().expect("processes array").is_empty(),
        "expected empty in-flight; got {env}",
    );
}

#[test]
fn kill_unknown_nonce_is_silent_ok() {
    let s = TESTING.harness();
    let env = s.kill("doesnotexist");
    assert!(
        env.is_null(),
        "a no-return kill of an unknown nonce yields no structured payload; got {env}",
    );
}

#[test]
fn plugin_path_resolves() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"path": "string"}),
        json!({"noop": 0}),
        "{ path: $nu.plugin-path }",
    );
    let path = env["result"]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("expected string; got {env}"));
    assert!(
        path.ends_with("plugin.msgpackz"),
        "expected $nu.plugin-path to end with plugin.msgpackz; got {path:?}",
    );
}

#[test]
fn tls_crypto_provider_installed() {
    let s = TESTING.harness();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "string"}),
        json!({"noop": 0}),
        "{ out: (try { http get 'https://127.0.0.1:9' | to text } catch {|e| $e.msg }) }",
    );
    let out = env["result"]["out"].as_str().unwrap_or_default();
    assert!(!out.is_empty(), "expected a connection error message; got {env}");
    assert!(
        !out.to_lowercase().contains("crypto provider"),
        "provider should be installed; got {out:?}",
    );
}

#[test]
fn multi_call_stability_and_scoping() {
    let s = TESTING.harness();
    for i in 0..10i64 {
        let env = s.run(
            json!({"x": "int"}),
            json!({"out": "int"}),
            json!({"x": i}),
            "{ out: ($args.x + 100) }",
        );
        assert_eq!(env["result"]["out"].as_i64(), Some(i + 100), "call {i}: got {env}");
    }
    // `scope commands` reports LOCAL scope as of nushell 0.115 (PR #18684), so an
    // eval always sees its own template `__run` and that name cannot witness
    // leakage. Probe with a name only THIS eval defines instead: visible to its
    // own eval, and absent from a later eval unless the shared base was polluted.
    let env = s.run(
        json!({"noop": "int"}),
        json!({"defined": "int"}),
        json!({"noop": 0}),
        "def __leak_probe [] { 0 }\n\
         { defined: (scope commands | where name == \"__leak_probe\" | length) }",
    );
    assert_eq!(
        env["result"]["defined"].as_i64(),
        Some(1),
        "a block-local def is visible to its own eval under 0.115 local scope; got {env}",
    );
    let env = s.run(
        json!({"noop": "int"}),
        json!({"leaked": "int"}),
        json!({"noop": 0}),
        "{ leaked: (scope commands | where name == \"__leak_probe\" | length) }",
    );
    assert_eq!(
        env["result"]["leaked"].as_i64(),
        Some(0),
        "an earlier eval's def must not reach a later eval's engine; got {env}",
    );
}
