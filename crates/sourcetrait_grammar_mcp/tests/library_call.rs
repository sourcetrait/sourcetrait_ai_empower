use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, has_error, valid_function_source, write_source};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
#[named]
fn call_after_commit_returns_result() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("calc");
    let _ = s.library("new", "sourcetrait/calc", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/calc:math:double"]);
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = s.commit("sourcetrait/calc");
    let env = s.call("sourcetrait/calc:math:double", json!({"x": 7}));
    assert_eq!(env["result"]["out"].as_i64(), Some(14), "got {env}");
    assert!(env.get("rerun_id").is_none(), "call envelope shouldn't echo rerun_id");
    assert!(env.get("version_id").is_none(), "call envelope shouldn't echo version_id");
}

#[test]
#[named]
fn call_after_module_commit_returns_result() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("importable");
    let _ = s.library("new", "sourcetrait/importable", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/importable:util:triple"]);
    write_source(
        &src,
        "util/triple/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 3) }"),
    );
    let _ = s.commit("sourcetrait/importable");
    let env = s.call("sourcetrait/importable:util:triple", json!({"x": 11}));
    assert_eq!(env["result"]["out"].as_i64(), Some(33), "got {env}");
}

#[test]
fn call_unknown_library_errors() {
    let s = TestServer::new();
    let env = s.call("sourcetrait/ghost:m:noop", json!({"noop": 0}));
    assert!(has_error(&env), "got {env}");
}

#[test]
#[named]
fn call_missing_function_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("partlib");
    let _ = s.library("new", "sourcetrait/partlib", src.to_str().unwrap());
    let env = s.call("sourcetrait/partlib:m:ghost", json!({"noop": 0}));
    assert!(has_error(&env), "got {env}");
}

#[test]
#[named]
fn call_bad_module_path_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("safelib");
    let _ = s.library("new", "sourcetrait/safelib", src.to_str().unwrap());
    for bad in [
        "sourcetrait/safelib:../etc:x",
        "sourcetrait/safelib:a/../b:x",
        "sourcetrait/safelib:/abs:x",
    ] {
        let env = s.call(bad, json!({"n": 0}));
        assert!(has_error(&env), "namepath {bad:?} should error; got {env}");
    }
}

#[test]
#[named]
fn call_args_typecheck_failure_surfaces() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("strictlib");
    let _ = s.library("new", "sourcetrait/strictlib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/strictlib:m:needs_int"]);
    write_source(
        &src,
        "m/needs_int/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = s.commit("sourcetrait/strictlib");
    let env = s.call("sourcetrait/strictlib:m:needs_int", json!({"x": "five"}));
    assert!(has_error(&env), "type mismatch should surface as error; got {env}");
}

#[test]
#[named]
fn inspect_returns_function_doc() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("inspectlib");
    let _ = s.library("new", "sourcetrait/inspectlib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/inspectlib:math:double"]);
    write_source(
        &src,
        "math/double/mod.nu",
        "# doubles its input\n#\n# returns the doubled value\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: ($args.x * 2) } }\n",
    );
    let _ = s.commit("sourcetrait/inspectlib");
    let env = s.inspect("sourcetrait/inspectlib:math:double");
    assert_eq!(env["summary"].as_str(), Some("doubles its input"));
    assert_eq!(env["details"].as_str(), Some("returns the doubled value"));
    assert_eq!(env["library"].as_str(), Some("sourcetrait/inspectlib"));
    assert_eq!(env["module_path"].as_str(), Some("math"));
    assert_eq!(env["name"].as_str(), Some("double"));
    assert_eq!(env["args_schema"], json!({"x": "int"}));
    assert_eq!(env["result_schema"], json!({"out": "int"}));
}

#[test]
#[named]
fn inspect_library_root_and_module() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("inspectlib2");
    let _ = s.library("new", "sourcetrait/inspectlib2", src.to_str().unwrap());
    write_source(&src, "mod.nu", "# the inspectlib2 library\nexport module math\n");
    write_source(&src, "math/mod.nu", "# math helpers\nexport use double\n");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = s.commit("sourcetrait/inspectlib2");
    let lib = s.inspect("sourcetrait/inspectlib2");
    assert_eq!(lib["summary"].as_str(), Some("the inspectlib2 library"));
    let m = s.inspect("sourcetrait/inspectlib2:math");
    assert_eq!(m["summary"].as_str(), Some("math helpers"));
}

#[test]
#[named]
fn inspect_undocumented_is_empty() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("inspectlib3");
    let _ = s.library("new", "sourcetrait/inspectlib3", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/inspectlib3:m:f"]);
    write_source(
        &src,
        "m/f/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = s.commit("sourcetrait/inspectlib3");
    let env = s.inspect("sourcetrait/inspectlib3:m:f");
    assert_eq!(env["summary"].as_str(), Some(""));
    assert_eq!(env["details"].as_str(), Some(""));
}

#[test]
fn inspect_unknown_library_errors() {
    let s = TestServer::new();
    let env = s.inspect("sourcetrait/ghost");
    assert!(has_error(&env), "got {env}");
}

#[test]
#[named]
fn result_record_field_shapes_preserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("fidelitylib");
    let _ = s.library("new", "sourcetrait/fidelitylib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/fidelitylib:m:shapes"]);
    write_source(
        &src,
        "m/shapes/mod.nu",
        "export def main [args: record<n: int>]: nothing -> record<p: path, d: directory, c: cell-path, g: glob> { { p: \"x\", d: \"y\", c: $.a, g: (\"z\" | into glob) } }\n",
    );
    let committed = s.commit("sourcetrait/fidelitylib");
    assert!(!has_error(&committed), "commit should succeed; got {committed}");
    let env = s.inspect("sourcetrait/fidelitylib:m:shapes");
    assert_eq!(
        env["result_schema"],
        json!({"p": "path", "d": "directory", "c": "cell-path", "g": "glob"}),
        "result fields must keep path/directory/cell-path/glob verbatim; got {}",
        env["result_schema"],
    );
    assert_eq!(env["args_schema"], json!({"n": "int"}));
}

#[test]
#[named]
fn helper_file_pruned_from_info_and_not_callable() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("helperlib");
    let _ = s.library("new", "sourcetrait/helperlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./util.nu\nexport use real\n");
    write_source(&src, "m/util.nu", "export def helper [n: int] { $n * 2 }\n");
    write_source(
        &src,
        "m/real/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1) }"),
    );
    let committed = s.commit("sourcetrait/helperlib");
    assert!(!has_error(&committed), "commit should succeed; got {committed}");

    let info = s.info();
    let libs = info["libraries"].as_array().expect("libraries array");
    let lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("sourcetrait/helperlib"))
        .expect("helperlib present in info() (not dropped by the helper file)");
    let module = lib["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .find(|m| m["name"].as_str() == Some("m"))
        .expect("module m present");
    let fn_names: Vec<&str> = module["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .map(|f| f["name"].as_str().expect("fn name"))
        .collect();
    assert_eq!(
        fn_names,
        vec!["real"],
        "only the call-target should be listed; got {fn_names:?}",
    );

    let ok = s.call("sourcetrait/helperlib:m:real", json!({"x": 41}));
    assert_eq!(ok["result"]["out"].as_i64(), Some(42), "got {ok}");

    let bad = s.call("sourcetrait/helperlib:m:util", json!({"n": 5}));
    assert!(
        has_error(&bad),
        "an organizational helper file must NOT be callable; got {bad}",
    );
}
