use indoc::indoc;
use sourcetrait_grammar_bed::{
    Bed, BedConfig, BedError, BedResult, Value, record,
};
use sourcetrait_testing::prelude::*;
use std::time::Duration;

static TESTING: testing::Module = testing::module!(Integration, {
    .using_temp_dir()
});

fn bed() -> Bed {
    Bed::new(BedConfig::default()).expect("build Bed")
}

/// Run source through a fresh default bed with no input and no args.
fn run(source: &str) -> BedResult<Value> {
    bed().run_script(source, "test.nu", None, &[])
}

/// A denied capability must fail BEFORE any effect: at parse, at compile
/// (unknown commands compile to a run-external call that does not exist), or
/// at eval - never succeed, never panic.
fn assert_denied(
    what: &str,
    result: BedResult<Value>,
) {
    match result {
        Err(BedError::Parse { .. })
        | Err(BedError::Compile { .. })
        | Err(BedError::Eval { .. }) => {}
        other => panic!("`{what}` must be denied, got: {other:?}"),
    }
}

#[tested]
fn filters_transform_values_through_in() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let input = Value::test_record(record! {
        "n" => Value::test_int(3),
        "name" => Value::test_string("soak"),
    });
    let out = bed()
        .run_script(
            "$in | update n {|row| $row.n * 2 } | update name {|row| $row.name | str upcase }",
            "test.nu",
            Some(input),
            &[],
        )
        .expect("filter run");
    let record = out.as_record().expect("record out");
    assert_eq!(record.get("n").expect("n").as_int().expect("int"), 6);
    assert_eq!(record.get("name").expect("name").as_str().expect("str"), "SOAK");
}

#[tested]
fn tables_round_trip_through_pipelines() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let input = Value::test_list(vec![
        Value::test_record(record! { "n" => Value::test_int(1), "name" => Value::test_string("a") }),
        Value::test_record(record! { "n" => Value::test_int(2), "name" => Value::test_string("b") }),
        Value::test_record(record! { "n" => Value::test_int(3), "name" => Value::test_string("c") }),
    ]);
    let out = bed()
        .run_script(
            "$in | where n > 1 | get name | str join \",\"",
            "test.nu",
            Some(input),
            &[],
        )
        .expect("table run");
    assert_eq!(out.as_str().expect("str"), "b,c");
}

#[tested]
fn main_binds_positional_args() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let source = indoc! {r#"
        def main [x: int, y: string] {
            { x: $x, y: $y }
        }
    "#};
    let args = [Value::test_int(7), Value::test_string("hi")];
    let out = bed()
        .run_script(source, "test.nu", None, &args)
        .expect("main run");
    let record = out.as_record().expect("record out");
    assert_eq!(record.get("x").expect("x").as_int().expect("int"), 7);
    assert_eq!(record.get("y").expect("y").as_str().expect("str"), "hi");
}

#[tested]
fn export_def_main_takes_a_record_arg_soak_draft_shape() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    // The .soak script draft's shape: export def main over a structured fill.
    let source = indoc! {r#"
        export def main [fill: record<agent: record<kind: string>>]: nothing -> record<mv: table<from: string, to: string>> {
            let instructions_filename = match $fill.agent.kind {
                "claude" => "CLAUDE.md"
                _ => "AGENTS.md"
            }

            {
                mv: [ { from: "INSTRUCTIONS.md", to: $instructions_filename } ]
            }
        }
    "#};
    let fill = Value::test_record(record! {
        "agent" => Value::test_record(record! { "kind" => Value::test_string("claude") }),
    });
    let out = bed()
        .run_script(source, "soak.after.nu", None, &[fill])
        .expect("draft-shape run");
    let mv = out
        .as_record()
        .expect("record out")
        .get("mv")
        .expect("mv")
        .as_list()
        .expect("mv table");
    let row = mv[0].as_record().expect("mv row");
    assert_eq!(row.get("from").expect("from").as_str().expect("str"), "INSTRUCTIONS.md");
    assert_eq!(row.get("to").expect("to").as_str().expect("str"), "CLAUDE.md");
}

#[tested]
fn main_receives_input_and_args_together() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let source = indoc! {r#"
        def main [prefix: string] {
            $in | each {|item| $"($prefix)($item)" }
        }
    "#};
    let input = Value::test_list(vec![Value::test_string("a"), Value::test_string("b")]);
    let out = bed()
        .run_script(source, "test.nu", Some(input), &[Value::test_string("x")])
        .expect("main with input");
    let items = out.as_list().expect("list out");
    assert_eq!(items[0].as_str().expect("str"), "xa");
    assert_eq!(items[1].as_str().expect("str"), "xb");
}

#[tested]
fn args_without_main_error() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let result = bed().run_script("$in", "test.nu", None, &[Value::test_int(1)]);
    match result {
        Err(BedError::MissingMain { count, .. }) => assert_eq!(count, 1),
        other => panic!("expected MissingMain, got: {other:?}"),
    }
}

#[tested]
fn main_arg_type_violations_are_runtime_eval_errors() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    // Args bind through variables, so nu checks them at RUNTIME - the failure
    // must surface as Eval (a catchable-shape error), not Parse.
    let result = bed().run_script(
        "def main [x: int] { $x }",
        "test.nu",
        None,
        &[Value::test_string("nope")],
    );
    match result {
        Err(BedError::Eval { .. }) => {}
        other => panic!("expected Eval, got: {other:?}"),
    }
}

#[tested]
fn denied_capabilities_all_fail() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let denied = [
        "open /etc/passwd",
        "ls",
        "^ls",
        "'x' | save /tmp/bed_denied.txt",
        "source foo.nu",
        "use foo.nu",
        "http get https://example.com",
        "sys host",
        "ps",
        "exit 1",
        "cd /",
        "sleep 1sec",
        "glob *",
        "path expand '.'",
        "'.' | path exists",
        "print 'leak'",
        "load-env {X: '1'}",
        "job spawn { 1 }",
    ];
    for snippet in denied {
        assert_denied(snippet, run(snippet));
    }
}

#[tested]
fn env_is_explicit_merge_in_only() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    // Config env is the ONLY environment the script sees.
    let configured = Bed::new(
        BedConfig::builder()
            .env("NUBED_TEST", Value::test_string("42"))
            .build(),
    )
    .expect("build Bed");
    let out = configured
        .run_script("$env.NUBED_TEST", "test.nu", None, &[])
        .expect("config env read");
    assert_eq!(out.as_str().expect("str"), "42");

    // The host process env never leaks in.
    unsafe { std::env::set_var("NUBED_HOST_LEAK", "leaked") };
    let out = configured
        .run_script("'NUBED_HOST_LEAK' in $env", "test.nu", None, &[])
        .expect("host env probe");
    assert!(!out.as_bool().expect("bool"), "host process env leaked into the script");
}

#[tested]
fn runs_are_hermetic() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let shared = bed();

    // A run may use $env internally...
    let out = shared
        .run_script("$env.NUBED_X = 'v'; $env.NUBED_X", "test.nu", None, &[])
        .expect("in-run env");
    assert_eq!(out.as_str().expect("str"), "v");
    // ...but nothing survives to the next run: not env...
    let out = shared
        .run_script("'NUBED_X' in $env", "test.nu", None, &[])
        .expect("env probe");
    assert!(!out.as_bool().expect("bool"), "$env leaked across runs");
    // ...and not defs.
    shared
        .run_script("def leaky [] { 1 }; leaky", "test.nu", None, &[])
        .expect("def in run 1");
    assert_denied("leaky (from a prior run)", shared.run_script("leaky", "test.nu", None, &[]));
}

#[tested]
fn errors_carry_their_phase() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    match run("def broken [") {
        Err(BedError::Parse { .. }) => {}
        other => panic!("expected Parse, got: {other:?}"),
    }
    match run("error make {msg: 'boom'}") {
        Err(BedError::Eval { message, .. }) => {
            assert!(message.contains("boom"), "message carries the detail: {message}")
        }
        other => panic!("expected Eval, got: {other:?}"),
    }
}

#[tested]
fn timeout_interrupts_a_hot_loop() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let bounded = Bed::new(
        BedConfig::builder()
            .timeout(Duration::from_millis(300))
            .build(),
    )
    .expect("build Bed");
    match bounded.run_script("loop {}", "test.nu", None, &[]) {
        Err(BedError::Timeout { .. }) => {}
        other => panic!("expected Timeout, got: {other:?}"),
    }
}

#[tested]
fn script_files_run_and_missing_files_error() {
    let test = testing::test!({
        .using_temp_dir()
    });
    let dir = test.temp_dir().to_path_buf();
    let script = dir.join("double.nu");
    std::fs::write(&script, "def main [n: int] { $n * 2 }").expect("write script");
    let out = bed()
        .run_script_file(&script, None, &[Value::test_int(21)])
        .expect("file run");
    assert_eq!(out.as_int().expect("int"), 42);

    match bed().run_script_file(&dir.join("absent.nu"), None, &[]) {
        Err(BedError::ScriptRead { .. }) => {}
        other => panic!("expected ScriptRead, got: {other:?}"),
    }
}

#[tested]
fn empty_output_is_nothing_and_included_nondeterminism_works() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let out = run("let x = 1").expect("no-output run");
    assert!(out.is_nothing(), "no-output script returns nothing: {out:?}");

    let out = run("(date now) | describe").expect("date now");
    assert_eq!(out.as_str().expect("str"), "datetime");
    let out = run("random int 5..5").expect("random int");
    assert_eq!(out.as_int().expect("int"), 5);
}

#[tested]
fn top_level_return_short_circuits_main() {
    let _test = testing::test!({
        .using_temp_dir()
    });
    let source = indoc! {r#"
        return "early"
        def main [] { "never" }
    "#};
    let out = bed().run_script(source, "test.nu", None, &[]).expect("early return");
    assert_eq!(out.as_str().expect("str"), "early");
}
