use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, has_error};

#[test]
fn env_mutation_persists_across_interact_calls() {
    let s = TestServer::new();
    let first = s.interact(
        json!({"value": "string"}),
        json!({"wrote": "string"}),
        json!({"value": "alpha"}),
        "$env.SHOT_DEMO = $args.value\n{ wrote: $args.value }",
    );
    assert_eq!(first["result"]["wrote"].as_str(), Some("alpha"));

    let second = s.interact(
        json!({"noop": "int"}),
        json!({"saw": "string"}),
        json!({"noop": 0}),
        "{ saw: $env.SHOT_DEMO }",
    );
    assert_eq!(
        second["result"]["saw"].as_str(),
        Some("alpha"),
        "the env mutation from call 1 should persist; got {second}",
    );
}

#[test]
fn cd_persists_across_interact_calls() {
    let s = TestServer::new();
    let _ = s.interact(
        json!({"target": "string"}),
        json!({"cwd": "string"}),
        json!({"target": "/tmp"}),
        "cd $args.target\n{ cwd: (pwd) }",
    );
    let second = s.interact(
        json!({"noop": "int"}),
        json!({"cwd": "string"}),
        json!({"noop": 0}),
        "{ cwd: $env.PWD }",
    );
    assert_eq!(second["result"]["cwd"].as_str(), Some("/tmp"), "cd should propagate; got {second}");
}

#[test]
fn interact_state_does_not_leak_into_run() {
    let s = TestServer::new();
    let _ = s.interact(
        json!({"noop": "int"}),
        json!({"ok": "bool"}),
        json!({"noop": 0}),
        "def leaked [] { 999 }\n{ ok: true }",
    );
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ out: (leaked) }",
    );
    assert!(has_error(&env), "run() must NOT see interact()'s `leaked` def; got {env}");
}

/// nushell's `EngineState::merge_env` ends by chdir'ing the process to `$env.PWD`.
/// In-process that would move the whole grammar host for the rest of its life, so the
/// interact lane merges env WITHOUT that sync (server/embed.rs merge_env_no_chdir) and
/// `cd` stays engine state. Linux-only, like the rest of the /proc-based checks.
#[test]
fn interact_cd_does_not_move_the_host_process_cwd() {
    let s = TestServer::new();
    let before = std::fs::read_link("/proc/self/cwd").expect("read /proc/self/cwd");
    let env = s.interact(
        json!({"dir": "string"}),
        json!({"pwd": "string"}),
        json!({"dir": "/etc"}),
        "cd $args.dir\n{ pwd: $env.PWD }",
    );
    assert_eq!(
        env["result"]["pwd"].as_str(),
        Some("/etc"),
        "cd should still set $env.PWD; got {env}",
    );
    let after = std::fs::read_link("/proc/self/cwd").expect("read /proc/self/cwd");
    assert_eq!(
        before, after,
        "an interact `cd` must not move the host process cwd - in-process there is no \
         worker subprocess to absorb it",
    );
}

#[test]
fn multi_line_body_with_command_then_record_parses() {
    let s = TestServer::new();
    let env = s.interact(
        json!({"x": "int"}),
        json!({"y": "int", "slept_ms": "int"}),
        json!({"x": 7}),
        "sleep 50ms\nlet doubled = ($args.x * 2)\n{ y: $doubled, slept_ms: 50 }",
    );
    assert_eq!(env["result"]["y"].as_i64(), Some(14));
    assert_eq!(env["result"]["slept_ms"].as_i64(), Some(50));
}
