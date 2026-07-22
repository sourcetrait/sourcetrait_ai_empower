use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_kind, has_error, valid_function_source, write_source};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

fn author_double_tree(src: &std::path::Path) {
    write_source(src, "mod.nu", "export module m\n");
    write_source(src, "m/mod.nu", "export use double\n");
    write_source(
        src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
}

#[test]
#[named]
fn install_brings_shipped_source_into_mcp() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("shiplib");
    author_double_tree(&src);
    let env = s.rig("install", "sourcetrait/shiplib", src.to_str().unwrap());
    assert!(!has_error(&env), "install should succeed; got {env}");
    assert!(
        !env["summary"]["added"].as_array().expect("added").is_empty(),
        "install summary should report added paths; got {env}",
    );
    assert!(
        s.rig_dir("sourcetrait/shiplib").exists(),
        "canonical should exist",
    );
    let called = s.call("sourcetrait/shiplib:m:double", json!({"x": 6}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(12),
        "installed rig should be callable; got {called}",
    );
}

#[test]
#[named]
fn install_rolls_back_on_validation_failure() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badship");
    write_source(&src, "mod.nu", "export use thing\n");
    write_source(
        &src,
        "thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let env = s.rig("install", "sourcetrait/badship", src.to_str().unwrap());
    assert_eq!(
        error_kind(&env),
        Some("rig::root_function"),
        "install of an invalid source should fail with a validation diagnostic; got {env}"
    );
    assert!(
        !s.rig_dir("sourcetrait/badship").exists(),
        "a failed install must leave nothing registered (canonical wiped)",
    );
    let re = s.rig("new", "sourcetrait/badship", src.to_str().unwrap());
    assert!(!has_error(&re), "name should be free after rollback; got {re}");
}

#[test]
#[named]
fn check_reports_ok_for_clean_source() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("checkoklib");
    let _ = s.rig("new", "sourcetrait/checkoklib", src.to_str().unwrap());
    author_double_tree(&src);
    let env = s.rig("check", "sourcetrait/checkoklib", src.to_str().unwrap());
    assert!(!has_error(&env), "check should not error; got {env}");
    let summary = &env["summary"];
    assert_eq!(summary["ok"].as_bool(), Some(true), "got {summary}");
    assert!(
        summary["errors"].as_array().expect("errors").is_empty(),
        "got {summary}"
    );
    assert!(
        summary["warnings"].as_array().expect("warnings").is_empty(),
        "got {summary}"
    );
}

#[test]
#[named]
fn check_reports_structural_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("checkerrlib");
    let _ = s.rig("new", "sourcetrait/checkerrlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export use thing\n");
    write_source(
        &src,
        "thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let env = s.rig("check", "sourcetrait/checkerrlib", src.to_str().unwrap());
    assert!(!has_error(&env), "check itself should not error; got {env}");
    let summary = &env["summary"];
    assert_eq!(summary["ok"].as_bool(), Some(false), "got {summary}");
    let err_kinds: Vec<&str> = summary["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(!err_kinds.is_empty(), "expected >=1 error; got {summary}");
    assert!(
        err_kinds.iter().any(|k| k.starts_with("rig::")),
        "errors should carry namespaced rig:: kinds; got {err_kinds:?}"
    );
}

#[test]
fn check_unregistered_rig_errors() {
    let s = TestServer::new();
    let env = s.rig("check", "sourcetrait/ghostlib", "/some/path");
    assert_eq!(
        error_kind(&env),
        Some("rig::not_registered"),
        "check requires a registered rig; got {env}"
    );
}

#[test]
#[named]
fn check_source_dir_mismatch_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("checkmmlib");
    let _ = s.rig("new", "sourcetrait/checkmmlib", src.to_str().unwrap());
    let env = s.rig("check", "sourcetrait/checkmmlib", "/wrong/path");
    assert_eq!(
        error_kind(&env),
        Some("rig::source_path_mismatch"),
        "check should cross-check source_dir; got {env}"
    );
}

#[test]
#[named]
fn uninstall_source_dir_mismatch_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("unmmlib");
    let _ = s.rig("new", "sourcetrait/unmmlib", src.to_str().unwrap());
    let env = s.rig("uninstall", "sourcetrait/unmmlib", "/wrong/path");
    assert_eq!(
        error_kind(&env),
        Some("rig::source_path_mismatch"),
        "uninstall should cross-check source_dir; got {env}"
    );
    assert!(
        s.rig_dir("sourcetrait/unmmlib").exists(),
        "a rejected uninstall must leave the rig registered",
    );
}

#[test]
fn invalid_action_errors() {
    let s = TestServer::new();
    let env = s.rig("frobnicate", "sourcetrait/x", "/p");
    assert_eq!(
        error_kind(&env),
        Some("rig::invalid_action"),
        "an unknown action should error; got {env}"
    );
}

#[test]
#[named]
fn run_body_can_use_a_committed_rig() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("uselib");
    let _ = s.rig("new", "sourcetrait/uselib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module calc\n");
    write_source(&src, "calc/mod.nu", "export use double\n");
    write_source(
        &src,
        "calc/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed = s.commit("sourcetrait/uselib");
    assert!(!has_error(&committed), "commit should succeed; got {committed}");

    let env = s.run(
        json!({}),
        json!({"out": "int"}),
        json!({}),
        "use rig/sourcetrait/uselib/calc\nlet r = (calc double {x: 5})\n{ out: $r.out }",
    );
    assert!(
        !has_error(&env),
        "a run() body should be able to `use` a committed rig; got {env}"
    );
    assert_eq!(
        env["result"]["out"].as_i64(),
        Some(10),
        "use-from-run should resolve + call the committed function; got {env}",
    );
}
