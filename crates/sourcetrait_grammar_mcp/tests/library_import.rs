use serde_json::json;
use sourcetrait_grammar_mcp::guts::{
    TestServer, error_kind, error_kinds, error_messages, error_text, has_error,
    valid_function_source, write_source,
};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
#[named]
fn commit_accepts_path_self_call_target() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("selflib");
    let _ = s.library("new", "sourcetrait/selflib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use whereami\n");
    write_source(
        &src,
        "m/whereami/mod.nu",
        "export def main [args: record<noop: int>]: nothing -> record<here: string> {\n    const SELF = (path self)\n    { here: $SELF }\n}\n",
    );
    let env = s.commit("sourcetrait/selflib");
    assert!(
        !has_error(&env),
        "a call-target using `path self` should commit; got {env}"
    );
}

#[test]
#[named]
fn commit_accepts_path_self_in_mod_nu_const() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("modselflib");
    let _ = s.library("new", "sourcetrait/modselflib", src.to_str().unwrap());
    write_source(
        &src,
        "mod.nu",
        "export const HERE = (path self)\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/modselflib");
    assert!(
        !has_error(&env),
        "a mod.nu `export const` using `path self` should commit; got {env}"
    );
}

#[test]
#[named]
fn commit_happy_path_writes_repo_and_meta() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("happylib");
    let _ = s.library("new", "sourcetrait/happylib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use double\n");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let env = s.commit("sourcetrait/happylib");
    assert!(!has_error(&env), "commit should succeed; got {env}");

    let lib = s.library_dir("sourcetrait/happylib");
    assert!(lib.join("mod.nu").exists());
    assert!(lib.join("math").join("mod.nu").exists());
    assert!(lib.join("math").join("double").join("mod.nu").exists());

    assert!(
        lib.join(".meta/library.nuon").exists(),
        "the index is NUON on disk, not JSON",
    );
    let meta = s.library_index("sourcetrait/happylib");
    assert_eq!(meta["source_path"].as_str(), Some(src.to_str().unwrap()));
    assert!(
        meta.get("kind").is_none(),
        "meta should not carry a kind field; got {meta}",
    );
}

#[test]
#[named]
fn commit_rejects_mod_nu_with_syntax_error() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badmodlib");
    let _ = s.library("new", "sourcetrait/badmodlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module foo\nexport\n");
    write_source(&src, "foo/mod.nu", "");
    let env = s.commit("sourcetrait/badmodlib");
    assert!(has_error(&env), "syntax-error mod.nu should reject; got {env}");
    let msg = error_text(&env);
    assert!(
        msg.contains("parse error"),
        "expected parse-error violation in mod.nu; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_rejects_mod_nu_referencing_missing_file() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("missingreflib");
    let _ = s.library("new", "sourcetrait/missingreflib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export use ./does_not_exist.nu\n");
    let env = s.commit("sourcetrait/missingreflib");
    assert!(has_error(&env), "missing-ref mod.nu should reject; got {env}");
    let msg = error_text(&env);
    assert!(
        msg.contains("ModuleNotFound") || msg.contains("does_not_exist"),
        "expected ModuleNotFound for missing ref; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_accepts_multiline_def_signature() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("multilinelib");
    let _ = s.library("new", "sourcetrait/multilinelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [\n    args: record<x: int>\n]: nothing -> record<out: int> {\n    { out: ($args.x * 2) }\n}\n",
    );
    let env = s.commit("sourcetrait/multilinelib");
    assert!(!has_error(&env), "multi-line signature should pass; got {env}");
}

#[test]
#[named]
fn commit_rejects_function_with_syntax_error() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("syntaxlib");
    let _ = s.library("new", "sourcetrait/syntaxlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module broken\n");
    write_source(
        &src,
        "m/broken/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> {\n    { out: ($args.x * 2) \n",
    );
    let env = s.commit("sourcetrait/syntaxlib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("parse error"),
        "expected parse error violation; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_rejects_main_without_output_type() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badlib1");
    let _ = s.library("new", "sourcetrait/badlib1", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<x: int>] { { out: $args.x } }\n",
    );

    let env = s.commit("sourcetrait/badlib1");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("output type"),
        "expected missing-output-type violation; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_accepts_call_target_with_helper_export() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("helperexportlib");
    let _ = s.library("new", "sourcetrait/helperexportlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def helper [n: int] { $n * 2 }\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );

    let env = s.commit("sourcetrait/helperexportlib");
    assert!(
        !has_error(&env),
        "a call-target may export helpers beside main; got {env}"
    );
    let called = s.call("sourcetrait/helperexportlib:m:thing", json!({"x": 5}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(5),
        "call() should run main; got {called}",
    );
}

#[test]
#[named]
fn commit_rejects_main_empty_record_output() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badlib3");
    let _ = s.library("new", "sourcetrait/badlib3", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<> { {} }\n",
    );

    let env = s.commit("sourcetrait/badlib3");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("unfleshed skeleton") || msg.contains("real fields"),
        "expected empty-output-record violation; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_accepts_mod_nu_with_inline_const() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("constmodlib");
    let _ = s.library("new", "sourcetrait/constmodlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module sub\nconst X = 42\n");
    write_source(&src, "sub/mod.nu", "");
    let env = s.commit("sourcetrait/constmodlib");
    assert!(
        !has_error(&env),
        "a private const in mod.nu is allowed; got {env}"
    );
}

#[test]
#[named]
fn commit_accepts_mod_nu_with_inline_alias() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("aliasmodlib");
    let _ = s.library("new", "sourcetrait/aliasmodlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module sub\nalias foo = ls\n");
    write_source(&src, "sub/mod.nu", "");
    let env = s.commit("sourcetrait/aliasmodlib");
    assert!(
        !has_error(&env),
        "a private alias in mod.nu is allowed; got {env}"
    );
}

#[test]
#[named]
fn commit_rejects_mod_nu_with_let() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("letmodlib");
    let _ = s.library("new", "sourcetrait/letmodlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "let x = 5\n");
    let env = s.commit("sourcetrait/letmodlib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("parse error") || msg.contains("Expected"),
        "expected parse error for let in mod.nu; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_accepts_mod_nu_with_only_comments() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("commentedmodlib");
    let _ = s.library("new", "sourcetrait/commentedmodlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "# this library is empty\n# more comment\n");
    let env = s.commit("sourcetrait/commentedmodlib");
    assert!(
        !has_error(&env),
        "empty mod.nu with comments should accept; got {env}"
    );
}

#[test]
#[named]
fn commit_accepts_mod_nu_with_inline_def() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badlib4");
    let _ = s.library("new", "sourcetrait/badlib4", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module sub\ndef helper [] { 99 }\n");
    write_source(&src, "sub/mod.nu", "");

    let env = s.commit("sourcetrait/badlib4");
    assert!(
        !has_error(&env),
        "a private def in mod.nu is allowed; got {env}"
    );
}

#[test]
#[named]
fn commit_aggregates_multiple_violations() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("badlib5");
    let _ = s.library("new", "sourcetrait/badlib5", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module a\nextern noise []\n");
    write_source(&src, "a/mod.nu", "export use skel\nexport use noout\n");
    write_source(
        &src,
        "a/skel/mod.nu",
        "export def main [args: record<>]: nothing -> record<out: int> { { out: 1 } }\n",
    );
    write_source(
        &src,
        "a/noout/mod.nu",
        "export def main [args: record<x: int>] { { out: $args.x } }\n",
    );

    let env = s.commit("sourcetrait/badlib5");
    let messages = error_messages(&env);
    assert!(
        messages.iter().any(|m| m.contains("mod.nu may only contain")),
        "got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("unfleshed skeleton")),
        "got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("output type")),
        "got {messages:?}"
    );
}

#[test]
#[named]
fn commit_caps_structural_violations() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("caplib");
    let _ = s.library("new", "sourcetrait/caplib", src.to_str().unwrap());
    write_source(
        &src,
        "mod.nu",
        "export module a\nextern e1 []\nextern e2 []\nextern e3 []\nextern e4 []\n",
    );
    write_source(&src, "a/mod.nu", "");
    let env = s.commit("sourcetrait/caplib");
    let errors = env["error"]["errors"].as_array().expect("errors array");
    assert_eq!(errors.len(), 3, "errors should cap at 3; got {errors:?}");
}

#[test]
#[named]
fn commit_rejects_root_call_target() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("rootfnlib");
    let _ = s.library("new", "sourcetrait/rootfnlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export use thing\n");
    write_source(
        &src,
        "thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/rootfnlib");
    assert!(
        error_kinds(&env).iter().any(|k| k == "library::root_function"),
        "expected library::root_function; got {:?}",
        error_kinds(&env),
    );
    let messages = error_messages(&env);
    assert!(
        messages.iter().any(|m| m.contains("library root") && m.contains("module")),
        "got {messages:?}"
    );
}

#[test]
#[named]
fn commit_succeeds_then_check_warns_long_summary() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("doclib");
    let _ = s.library("new", "sourcetrait/doclib", src.to_str().unwrap());
    let long = "x".repeat(81);
    write_source(&src, "mod.nu", &format!("# {long}\nexport module m\n"));
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let committed = s.commit("sourcetrait/doclib");
    assert!(
        !has_error(&committed),
        "an over-long summary is advisory; commit should succeed; got {committed}"
    );

    let check = s.library("check", "sourcetrait/doclib", src.to_str().unwrap());
    let summary = &check["summary"];
    assert_eq!(
        summary["ok"].as_bool(),
        Some(true),
        "warnings don't fail check; got {check}"
    );
    let warn_kinds: Vec<&str> = summary["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .filter_map(|w| w["kind"].as_str())
        .collect();
    assert!(
        warn_kinds.iter().any(|k| *k == "lint::summary_length"),
        "expected lint::summary_length warning; got {warn_kinds:?}"
    );
}

#[test]
#[named]
fn commit_accepts_short_summary() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("okdoclib");
    let _ = s.library("new", "sourcetrait/okdoclib", src.to_str().unwrap());
    write_source(
        &src,
        "mod.nu",
        "# doubles its input\n# the math double helper module\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/okdoclib");
    assert!(
        !has_error(&env),
        "documented library should commit; got {env}"
    );
}

#[test]
#[named]
fn library_new_reestablish_duplicate_errors() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("duplib");
    let r1 = s.library("new", "sourcetrait/duplib", src.to_str().unwrap());
    assert!(!has_error(&r1), "first library(new) should succeed; got {r1}");
    let r2 = s.library("new", "sourcetrait/duplib", src.to_str().unwrap());
    assert!(has_error(&r2), "duplicate establish should error; got {r2}");
    assert_eq!(
        error_kind(&r2),
        Some("library::already_registered"),
        "got {r2}"
    );
}

#[test]
#[named]
fn commit_picks_up_mutated_source() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("livelib");
    let _ = s.library("new", "sourcetrait/livelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = s.commit("sourcetrait/livelib");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1000) }"),
    );
    let env = s.commit("sourcetrait/livelib");
    assert!(!has_error(&env), "re-commit should succeed; got {env}");
    let committed = std::fs::read_to_string(
        s.library_dir("sourcetrait/livelib").join("m").join("thing").join("mod.nu"),
    )
    .unwrap();
    assert!(
        committed.contains("+ 1000"),
        "committed body should reflect mutated source; got {committed:?}",
    );
}

#[test]
fn commit_unknown_library_errors() {
    let s = TestServer::new();
    let env = s.commit("sourcetrait/ghost");
    assert!(has_error(&env), "got {env}");
}

#[test]
#[named]
fn commit_validates_by_name_cross_library_use() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();

    let base = t.temp_dir().join("baselib");
    let _ = s.library("new", "sourcetrait/baselib", base.to_str().unwrap());
    write_source(&base, "mod.nu", "export module m\n");
    write_source(&base, "m/mod.nu", "export use double\n");
    write_source(
        &base,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed_base = s.commit("sourcetrait/baselib");
    assert!(
        !has_error(&committed_base),
        "baselib commit should succeed; got {committed_base}"
    );

    let consumer = t.temp_dir().join("consumer");
    let _ = s.library("new", "sourcetrait/consumer", consumer.to_str().unwrap());
    write_source(&consumer, "mod.nu", "export module app\n");
    write_source(&consumer, "app/mod.nu", "export use compute\n");
    write_source(
        &consumer,
        "app/compute/mod.nu",
        "use rig/sourcetrait/baselib/m\nexport def main [args: record<x: int>]: nothing -> record<out: int> {\n    m double {x: $args.x}\n}\n",
    );
    let committed = s.commit("sourcetrait/consumer");
    assert!(
        !has_error(&committed),
        "a library using a committed sibling by name should commit; got {committed}"
    );

    let called = s.call("sourcetrait/consumer:app:compute", json!({"x": 5}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(10),
        "cross-library call-target should resolve + run; got {called}",
    );
}

#[test]
#[named]
fn commit_and_call_resolves_authored_self_ref() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("selfreflib");
    let _ = s.library("new", "sourcetrait/selfreflib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module base\nexport module top\n");
    write_source(&src, "base/mod.nu", "export def val []: nothing -> int { 21 }\n");
    write_source(&src, "top/mod.nu", "export use double\n");
    write_source(
        &src,
        "top/double/mod.nu",
        "use rig/sourcetrait/selfreflib/base\nexport def main [args: nothing]: nothing -> record<out: int> {\n    { out: ((base val) * 2) }\n}\n",
    );
    let committed = s.commit("sourcetrait/selfreflib");
    assert!(
        !has_error(&committed),
        "a library with an authored self-ref target must validate at commit; got {committed}"
    );
    let called = s.call("sourcetrait/selfreflib:top:double", json!({}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(42),
        "the authored self-ref must resolve + run at serve; got {called}"
    );
}

#[test]
#[named]
fn commit_accepts_organizational_file() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("orglib");
    let _ = s.library("new", "sourcetrait/orglib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(
        &src,
        "util.nu",
        "export const LIMIT = 10\nexport def helper [n: int] { $n * 2 }\n",
    );
    let env = s.commit("sourcetrait/orglib");
    assert!(
        !has_error(&env),
        "organizational file should commit; got {env}"
    );
}

#[test]
#[named]
fn commit_accepts_mod_nu_with_export_const_and_def() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("modutillib");
    let _ = s.library("new", "sourcetrait/modutillib", src.to_str().unwrap());
    write_source(
        &src,
        "mod.nu",
        "export const VERSION = 1\nexport def shared [] { 42 }\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/modutillib");
    assert!(
        !has_error(&env),
        "mod.nu with export const/def should commit; got {env}"
    );
}

#[test]
#[named]
fn commit_rejects_empty_record_skeleton() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("skellib");
    let _ = s.library("new", "sourcetrait/skellib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<>]: nothing -> record<n: int> { { n: 1 } }\n",
    );
    let env = s.commit("sourcetrait/skellib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("unfleshed skeleton") || msg.contains("real fields"),
        "expected skeleton-rejection violation; got {msg:?}",
    );
}

#[test]
#[named]
fn commit_rejects_private_def_named_reserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("pdeflib");
    let _ = s.library("new", "sourcetrait/pdeflib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(&src, "util.nu", "export const LIMIT = 5\ndef main [] { 1 }\n");
    let env = s.commit("sourcetrait/pdeflib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("reserved"),
        "expected reserved-term violation; got {msg:?}"
    );
}

#[test]
#[named]
fn commit_rejects_module_named_reserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("modreslib");
    let _ = s.library("new", "sourcetrait/modreslib", src.to_str().unwrap());
    std::fs::create_dir_all(src.join("main")).unwrap();
    write_source(&src, "mod.nu", "export module main\n");
    write_source(&src, "main/mod.nu", "");
    let env = s.commit("sourcetrait/modreslib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("reserved"),
        "expected reserved-name violation; got {msg:?}"
    );
}

#[test]
#[named]
fn commit_rejects_const_named_reserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("constreslib");
    let _ = s.library("new", "sourcetrait/constreslib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export const main = 5\nexport module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/constreslib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("reserved"),
        "expected reserved-const violation; got {msg:?}"
    );
}

#[test]
#[named]
fn commit_rejects_record_key_reserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("rkeylib");
    let _ = s.library("new", "sourcetrait/rkeylib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x, main: 1 } }\n",
    );
    let env = s.commit("sourcetrait/rkeylib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("reserved"),
        "expected reserved record-key violation; got {msg:?}"
    );
}

#[test]
#[named]
fn commit_rejects_cellpath_member_reserved() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("cpathlib");
    let _ = s.library("new", "sourcetrait/cpathlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.main } }\n",
    );
    let env = s.commit("sourcetrait/cpathlib");
    assert!(has_error(&env));
    let msg = error_text(&env);
    assert!(
        msg.contains("reserved"),
        "expected reserved cell-path violation; got {msg:?}"
    );
}

#[test]
#[named]
fn commit_accepts_reserved_as_quoted_string_value() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("strvallib");
    let _ = s.library("new", "sourcetrait/strvallib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { let note = \"main\"; { out: $args.x } }\n",
    );
    let env = s.commit("sourcetrait/strvallib");
    assert!(
        !has_error(&env),
        "quoted string value should not trip the ban; got {env}"
    );
}

#[test]
#[named]
fn library_new_and_scaffold_function() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("scaffolded");
    let r1 = s.library("new", "sourcetrait/scaffolded", src.to_str().unwrap());
    assert!(!has_error(&r1), "establish should succeed; got {r1}");
    let meta = s.library_index("sourcetrait/scaffolded");
    assert_eq!(
        meta["source_path"].as_str(),
        Some(src.to_str().unwrap()),
        "meta should record source_path; got {meta}",
    );
    assert!(
        src.join("mod.nu").exists(),
        "source root mod.nu should be seeded"
    );

    let r2 = s.scaffold(&["sourcetrait/scaffolded:math:double"]);
    assert!(!has_error(&r2), "scaffold function should succeed; got {r2}");
    let fn_src = std::fs::read_to_string(src.join("math").join("double").join("mod.nu")).unwrap();
    assert!(
        fn_src.contains("export def main [args: record<>]: nothing -> record<>"),
        "skeleton missing the single infix-signatured main; got {fn_src:?}"
    );
    assert!(
        !fn_src.contains("export def call") && !fn_src.contains("export def resolve"),
        "skeleton should be 1-def main only; got {fn_src:?}"
    );
    let math_mod = std::fs::read_to_string(src.join("math").join("mod.nu")).unwrap();
    assert!(
        math_mod.contains("export module double"),
        "math mod.nu should DECLARE the call as a submodule; got {math_mod:?}",
    );
    assert!(
        math_mod.contains("export use double"),
        "math mod.nu should RE-EXPORT the call (0.114 no longer implicitly imports \
         submodules, so `export module` alone leaves it unreachable); got {math_mod:?}",
    );
    let root_mod = std::fs::read_to_string(src.join("mod.nu")).unwrap();
    assert!(
        root_mod.contains("export module math"),
        "root mod.nu should wire math; got {root_mod:?}",
    );
    assert!(
        !root_mod.contains("export use math"),
        "`math` is a PURE module (no `main`): an `export use` would flatten its \
         helpers into the parent; got {root_mod:?}",
    );
}

#[test]
#[named]
fn scaffolded_call_commits_as_wired() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("wiredlib");
    let _ = s.library("new", "sourcetrait/wiredlib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/wiredlib:math:double"]);
    std::fs::write(
        src.join("math").join("double").join("mod.nu"),
        valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    )
    .unwrap();
    let env = s.commit("sourcetrait/wiredlib");
    assert!(
        !has_error(&env),
        "the scaffolder's own wiring must pass the validator; got {env}"
    );
    let called = s.call("sourcetrait/wiredlib:math:double", json!({"x": 21}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(42),
        "a scaffolded call must be drivable; got {called}",
    );
}

#[test]
#[named]
fn new_leaf_guard_refuses_existing_function() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("guarded");
    let _ = s.library("new", "sourcetrait/guarded", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/guarded:m:f"]);
    let dup = s.scaffold(&["sourcetrait/guarded:m:f"]);
    assert!(
        has_error(&dup),
        "scaffolding over an existing function should reject; got {dup}"
    );
}

#[test]
fn scaffold_into_unregistered_library_errors() {
    let s = TestServer::new();
    let env = s.scaffold(&["sourcetrait/nopath:m:f"]);
    assert!(
        has_error(&env),
        "scaffolding into an unregistered library should reject; got {env}"
    );
    assert_eq!(
        error_kind(&env),
        Some("library::not_registered"),
        "got {env}"
    );
}

#[test]
#[named]
fn commit_validates_and_upserts_source() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("clib");
    let _ = s.library("new", "sourcetrait/clib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/clib:math:double"]);
    std::fs::write(
        src.join("math").join("double").join("mod.nu"),
        valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    )
    .unwrap();
    let env = s.commit("sourcetrait/clib");
    assert!(!has_error(&env), "commit should succeed; got {env}");
    assert!(
        s.library_dir("sourcetrait/clib")
            .join("math")
            .join("double")
            .join("mod.nu")
            .exists(),
        "canonical should have the committed function",
    );
    let env2 = s.commit("sourcetrait/clib");
    assert!(!has_error(&env2), "no-change commit should succeed; got {env2}");
    assert!(
        env2["added"].as_array().expect("added").is_empty()
            && env2["modified"].as_array().expect("modified").is_empty()
            && env2["removed"].as_array().expect("removed").is_empty(),
        "no-change commit should report nothing changed; got {env2}",
    );
}

#[test]
#[named]
fn commit_rejects_unfleshed_skeleton() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("sklib");
    let _ = s.library("new", "sourcetrait/sklib", src.to_str().unwrap());
    let _ = s.scaffold(&["sourcetrait/sklib:m:raw"]);
    let env = s.commit("sourcetrait/sklib");
    assert!(
        has_error(&env),
        "committing an unfleshed skeleton should reject; got {env}"
    );
}

#[test]
#[named]
fn commit_rejects_main_in_flat_file() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("flatmainlib");
    let _ = s.library("new", "sourcetrait/flatmainlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./impl.nu\n");
    write_source(
        &src,
        "m/impl.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let env = s.commit("sourcetrait/flatmainlib");
    assert!(
        error_kinds(&env).iter().any(|k| k == "library::main_in_flat_file"),
        "expected library::main_in_flat_file; got {:?}",
        error_kinds(&env),
    );
}

#[test]
#[named]
fn commit_rejects_call_wired_via_export_module() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("wirelib");
    let _ = s.library("new", "sourcetrait/wirelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module double\n");
    write_source(
        &src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let env = s.commit("sourcetrait/wirelib");
    assert!(
        error_kinds(&env).iter().any(|k| k == "library::call_wiring"),
        "expected library::call_wiring; got {:?}",
        error_kinds(&env),
    );
}

#[test]
#[named]
fn commit_rejects_call_with_submodule() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("leaflib");
    let _ = s.library("new", "sourcetrait/leaflib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use call\n");
    write_source(
        &src,
        "m/call/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    write_source(&src, "m/call/sub/mod.nu", "");
    let env = s.commit("sourcetrait/leaflib");
    assert!(
        error_kinds(&env).iter().any(|k| k == "library::call_leaf"),
        "expected library::call_leaf; got {:?}",
        error_kinds(&env),
    );
}

#[test]
#[named]
fn commit_rejects_orphan_module() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("orphanlib");
    let _ = s.library("new", "sourcetrait/orphanlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use double\n");
    write_source(
        &src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    write_source(&src, "m/stray/mod.nu", "");
    let env = s.commit("sourcetrait/orphanlib");
    assert!(
        error_kinds(&env).iter().any(|k| k == "library::orphan"),
        "expected library::orphan; got {:?}",
        error_kinds(&env),
    );
}

#[test]
#[named]
fn commit_accepts_nu_cmd_extra_command() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("extralib");
    let _ = s.library("new", "sourcetrait/extralib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module snake\nexport use snake\n");
    write_source(
        &src,
        "m/snake/mod.nu",
        "export def main [args: record<text: string>]: nothing -> record<out: string> {\n    { out: ($args.text | str snake-case) }\n}\n",
    );
    let env = s.commit("sourcetrait/extralib");
    assert!(
        !has_error(&env),
        "a call-target using nu-cmd-extra (`str snake-case`) must commit; got {env}"
    );
    let called = s.call("sourcetrait/extralib:m:snake", json!({"text": "Hello World"}));
    assert_eq!(
        called["result"]["out"].as_str(),
        Some("hello_world"),
        "the committed nu-cmd-extra call must run; got {called}",
    );
}

#[test]
#[named]
fn commit_accepts_call_with_flat_helper() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("callhelperlib");
    let _ = s.library("new", "sourcetrait/callhelperlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(&src, "m/thing/helper.nu", "export def doubler [n: int] { $n * 2 }\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        "use ./helper.nu *\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: (doubler $args.x) } }\n",
    );
    let env = s.commit("sourcetrait/callhelperlib");
    assert!(
        !has_error(&env),
        "a call may use a flat helper sibling; got {env}"
    );
    let called = s.call("sourcetrait/callhelperlib:m:thing", json!({"x": 5}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(10),
        "call should run with the flat helper; got {called}",
    );
}
