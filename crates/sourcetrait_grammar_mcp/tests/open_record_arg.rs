use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_text, has_error, write_tree};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
fn run_binds_open_record_arg() {
    let s = TestServer::new();
    let env = s.run(
        json!({"fill": {}}),
        json!({"cols": "int"}),
        json!({"fill": {"a": 1, "b": "x"}}),
        "{ cols: ($args.fill | columns | length) }",
    );
    assert_eq!(env["result"]["cols"].as_i64(), Some(2), "open-record arg should bind an arbitrary record; got {env}");
}

#[test]
fn run_binds_empty_open_record_arg() {
    let s = TestServer::new();
    let env = s.run(
        json!({"fill": {}}),
        json!({"cols": "int"}),
        json!({"fill": {}}),
        "{ cols: ($args.fill | columns | length) }",
    );
    assert_eq!(env["result"]["cols"].as_i64(), Some(0), "got {env}");
}

#[test]
fn run_rejects_non_record_open_arg() {
    let s = TestServer::new();
    let env = s.run(
        json!({"fill": {}}),
        json!({"cols": "int"}),
        json!({"fill": 5}),
        "{ cols: 0 }",
    );
    assert!(has_error(&env), "a non-record `fill` should be rejected; got {env}");
}

#[test]
fn run_result_open_record_still_denied() {
    let s = TestServer::new();
    let env = s.run(
        json!({"noop": "int"}),
        json!({"fill": {}}),
        json!({"noop": 0}),
        "{ fill: {} }",
    );
    assert!(has_error(&env), "result open record should deny; got {env}");
    assert!(error_text(&env).contains("nested empty record"), "expected the nested-empty-record denial; got {env}");
}

#[test]
#[named]
fn commit_inspect_and_call_open_record_arg_field() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("openlib");
    let _ = s.library("new", "sourcetrait/openlib", src.to_str().unwrap());
    write_tree(
        &src,
        &[
            ("mod.nu", "export module m\n"),
            ("m/mod.nu", "export use soak\n"),
            (
                "m/soak/mod.nu",
                "export def main [args: record<x: int, fill: record<>>]: nothing -> record<sum: int, fillcols: int> {\n    { sum: $args.x, fillcols: ($args.fill | columns | length) }\n}\n",
            ),
        ],
    );
    let committed = s.commit("sourcetrait/openlib");
    assert!(!has_error(&committed), "a call-target with a `record<>` arg field should commit; got {committed}");

    let inspected = s.inspect("sourcetrait/openlib:m:soak");
    assert_eq!(inspected["args_schema"], json!({"x": "int", "fill": {}}), "inspect should show the open-record arg field; got {inspected}");
    assert_eq!(inspected["result_schema"], json!({"sum": "int", "fillcols": "int"}));

    let called = s.call("sourcetrait/openlib:m:soak", json!({"x": 5, "fill": {"a": 1, "b": 2, "c": 3}}));
    assert_eq!(called["result"]["sum"].as_i64(), Some(5), "got {called}");
    assert_eq!(called["result"]["fillcols"].as_i64(), Some(3), "got {called}");
}
