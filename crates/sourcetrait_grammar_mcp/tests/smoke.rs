//! Domain smoke tests, driven in-process via `guts::TestServer`. The raw-stdio
//! transport (the JSON-RPC handshake + tools/list framing) is a SYSTEM concern
//! covered by `sourcetrait_grammar_tests` (host_run_tool); these exercise what
//! run / processes / kill actually DO.

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_kind, error_text, has_error};

#[test]
fn arg_typecheck_error_surfaces() {
    let s = TestServer::new();
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
    let s = TestServer::new();
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
    let s = TestServer::new();
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
    let s = TestServer::new();
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
    let s = TestServer::new();
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
    let s = TestServer::new();
    let env = s.processes();
    assert!(
        env["processes"].as_array().expect("processes array").is_empty(),
        "expected empty in-flight; got {env}",
    );
}

#[test]
fn kill_unknown_nonce_is_silent_ok() {
    let s = TestServer::new();
    let env = s.kill("doesnotexist");
    assert!(
        env.is_null(),
        "a no-return kill of an unknown nonce yields no structured payload; got {env}",
    );
}

#[test]
fn plugin_path_resolves() {
    let s = TestServer::new();
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
    let s = TestServer::new();
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
    let s = TestServer::new();
    for i in 0..10i64 {
        let env = s.run(
            json!({"x": "int"}),
            json!({"out": "int"}),
            json!({"x": i}),
            "{ out: ($args.x + 100) }",
        );
        assert_eq!(env["result"]["out"].as_i64(), Some(i + 100), "call {i}: got {env}");
    }
    let env = s.run(
        json!({"noop": "int"}),
        json!({"leaked": "int"}),
        json!({"noop": 0}),
        "{ leaked: (scope commands | where name == \"__run\" | length) }",
    );
    assert_eq!(
        env["result"]["leaked"].as_i64(),
        Some(0),
        "do-block scoping should keep __run out of the persistent EngineState; got {env}",
    );
}
