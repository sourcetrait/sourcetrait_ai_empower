use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, has_kind};

#[test]
fn lint_rejects_run_hardcoded_path() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ p: \"/home/box/proj/x\", out: 0 }",
    );
    assert!(has_kind(&env, "lint::hardcoded_variable"), "got {env}");
}

#[test]
fn lint_rejects_run_denied_external() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ x: (^awk '{print $1}' | str trim), out: 0 }",
    );
    assert!(has_kind(&env, "lint::denied_command"), "got {env}");
}

#[test]
fn lint_passes_clean_run() {
    let s = TestServer::new();
    let env = s.run(
        json!({"x": "int"}),
        json!({"out": "int"}),
        json!({"x": 5}),
        "{ out: ($args.x + 1) }",
    );
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}

#[test]
fn lint_aggregates_multiple_violations() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "^awk 'x'\ncd \"/a/b\"\n{ out: 0 }",
    );
    assert!(has_kind(&env, "lint::denied_command"), "got {env}");
    assert!(has_kind(&env, "lint::hardcoded_variable"), "got {env}");
}

#[test]
fn lint_interact_rejects_hardcoded_path() {
    let s = TestServer::new();
    let env = s.interact(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ p: \"/home/box/x\", out: 0 }",
    );
    assert!(has_kind(&env, "lint::hardcoded_variable"), "got {env}");
}

#[test]
fn lint_interact_rejects_denied_external() {
    let s = TestServer::new();
    let env = s.interact(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ x: (^awk 'x' | str trim), out: 0 }",
    );
    assert!(has_kind(&env, "lint::denied_command"), "got {env}");
}
