use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
#[named]
fn cli_info_prints_json() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(t.temp_dir(), &["--id", "cid", "cli", "info"]);
    assert!(out.status.success(), "cli info should exit 0; got {out:?}");
    let v = stdout_json(&out);
    assert_eq!(v["name"].as_str(), Some("grammar"), "got {v}");
    assert_eq!(v["id"].as_str(), Some("cid"), "got {v}");
    assert_eq!(v["namespace"].as_str(), Some("default"), "got {v}");
    assert!(
        v["nu_version"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "info JSON should carry nu_version; got {v}",
    );
}

#[test]
#[named]
fn cli_rig_lifecycle_and_call() {
    let t = testing::test!({ .using_temp_dir() });
    let src = t.temp_dir().join("src").join("clilib");
    let src_str = src.to_str().unwrap();

    let established = run_output(
        t.temp_dir(),
        &["--id", "cid", "cli", "rig", "new", "sourcetrait/clilib", src_str],
    );
    assert!(
        established.status.success(),
        "cli rig new should exit 0; got {established:?}",
    );

    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use double\n");
    write_source(
        &src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let committed = run_output(t.temp_dir(), &["--id", "cid", "cli", "commit", "sourcetrait/clilib"]);
    assert!(
        committed.status.success(),
        "cli commit should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&committed),
        String::from_utf8_lossy(&committed.stderr),
    );

    let called = run_output(
        t.temp_dir(),
        &["--id", "cid", "cli", "call", "sourcetrait/clilib:m:double", "{x: 21}"],
    );
    assert!(
        called.status.success(),
        "cli call should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&called),
        String::from_utf8_lossy(&called.stderr),
    );
    let v = stdout_json(&called);
    assert_eq!(v["result"]["out"].as_i64(), Some(42), "got {v}");
}

#[test]
#[named]
fn cli_run_evaluates_nuon_schemas_and_args() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(
        t.temp_dir(),
        &[
            "--id", "cid", "cli", "run", "--args-schema", "{x: int}", "--result-schema",
            "{out: int}", "--args", "{x: 5}", "{ out: ($args.x + 1) }",
        ],
    );
    assert!(
        out.status.success(),
        "cli run should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&out),
        String::from_utf8_lossy(&out.stderr),
    );
    let v = stdout_json(&out);
    assert_eq!(v["result"]["out"].as_i64(), Some(6), "got {v}");
    assert!(v["nonce"].as_str().is_some(), "run envelope should carry a nonce; got {v}");
}

#[test]
#[named]
fn cli_error_envelope_exits_one() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(
        t.temp_dir(),
        &["--id", "cid", "cli", "call", "sourcetrait/ghost:m:noop", "{}"],
    );
    assert_eq!(out.status.code(), Some(1), "an error envelope should exit 1; got {out:?}");
    let v = stdout_json(&out);
    assert!(v.get("error").is_some(), "the error envelope should print as JSON; got {v}");
}

#[test]
#[named]
fn cli_output_is_bare_compact_json() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(t.temp_dir(), &["--id", "cid", "cli", "info"]);
    assert!(out.status.success(), "cli info should exit 0; got {out:?}");
    let text = stdout_str(&out);
    assert!(!text.contains('\u{1b}'), "stdout must carry no ANSI escapes; got {text:?}");
    let trimmed = text.trim();
    assert!(!trimmed.contains('\n'), "compact JSON is a single line; got {text:?}");
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "stdout should be exactly one JSON object; got {text:?}",
    );
}

#[test]
#[named]
fn cli_kill_prints_nothing_and_exits_zero() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(t.temp_dir(), &["--id", "cid", "cli", "kill", "doesnotexist"]);
    assert!(out.status.success(), "cli kill should exit 0; got {out:?}");
    assert!(
        stdout_str(&out).trim().is_empty(),
        "no-return tools print nothing; got {:?}",
        stdout_str(&out),
    );
}
