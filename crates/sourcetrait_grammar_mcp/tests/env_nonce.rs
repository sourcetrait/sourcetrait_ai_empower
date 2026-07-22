use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, has_error, write_tree};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
fn run_body_sees_nonce_matching_envelope() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"seen": "string"}),
        json!({"noop": 0}),
        "{ seen: $env.NONCE }",
    );
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert!(!nonce.is_empty(), "envelope nonce should be non-empty");
    assert_eq!(env["result"]["seen"].as_str(), Some(nonce), "run body's $env.NONCE should equal the envelope nonce; got {env}");
}

#[test]
fn run_nested_helper_inherits_nonce() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"seen": "string"}),
        json!({"noop": 0}),
        "def grab [] { $env.NONCE }\n{ seen: (grab) }",
    );
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(env["result"]["seen"].as_str(), Some(nonce), "a nested helper should inherit $env.NONCE; got {env}");
}

#[test]
#[named]
fn call_target_sees_nonce() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("noncelib");
    let _ = s.rig("new", "sourcetrait/noncelib", src.to_str().unwrap());
    write_tree(
        &src,
        &[
            ("mod.nu", "export module m\n"),
            ("m/mod.nu", "export use whoami\n"),
            (
                "m/whoami/mod.nu",
                "export def main [args: record<noop: int>]: nothing -> record<seen: string> { { seen: $env.NONCE } }\n",
            ),
        ],
    );
    let committed = s.commit("sourcetrait/noncelib");
    assert!(!has_error(&committed), "commit should succeed; got {committed}");
    let env = s.call("sourcetrait/noncelib:m:whoami", json!({"noop": 0}));
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(env["result"]["seen"].as_str(), Some(nonce), "a committed call-target should read $env.NONCE; got {env}");
}

#[test]
fn interact_body_sees_nonce() {
    let s = TestServer::new();
    let env = s.interact(
        json!({"noop": "int"}),
        json!({"seen": "string"}),
        json!({"noop": 0}),
        "{ seen: $env.NONCE }",
    );
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(env["result"]["seen"].as_str(), Some(nonce), "interact body's $env.NONCE should equal the envelope nonce; got {env}");
}

#[test]
fn interact_nonce_is_fresh_per_call_and_keeps_env_persistence() {
    let s = TestServer::new();
    let a = s.interact(
        json!({"noop": "int"}),
        json!({"seen": "string"}),
        json!({"noop": 0}),
        "$env.KEEP = $env.NONCE\n{ seen: $env.NONCE }",
    );
    let nonce_a = a["nonce"].as_str().expect("A nonce").to_string();
    assert_eq!(a["result"]["seen"].as_str(), Some(nonce_a.as_str()));

    let b = s.interact(
        json!({"noop": "int"}),
        json!({"seen": "string", "keep": "string"}),
        json!({"noop": 0}),
        "{ seen: $env.NONCE, keep: $env.KEEP }",
    );
    let nonce_b = b["nonce"].as_str().expect("B nonce").to_string();
    assert_ne!(nonce_a, nonce_b, "each call gets a distinct nonce");
    assert_eq!(b["result"]["seen"].as_str(), Some(nonce_b.as_str()), "B sees B's nonce; got {b}");
    assert_eq!(b["result"]["keep"].as_str(), Some(nonce_a.as_str()), "A's $env.KEEP write should persist into B; got {b}");
}
