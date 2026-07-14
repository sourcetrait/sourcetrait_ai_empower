//! Unit tests for `crate::template` (the run / call / interact source
//! builders): golden-string equality + a clean-parse gate.

use crate::*;

fn obj(s: &str) -> mcp::JsonObject {
    match serde_json::from_str::<json::Value>(s).unwrap() {
        json::Value::Object(m) => m,
        _ => panic!("not an object"),
    }
}

/// Parse the rendered source on the full-shell lint engine; a clean parse (no
/// parse errors) is the gate the golden strings must clear.
fn parses_clean(src: &str) -> bool {
    let engine = ParseEngine::new_full();
    let mut ws = nu::StateWorkingSet::new(engine.engine_state());
    let _ = nu::parse(&mut ws, Some("golden.nu"), src.as_bytes(), false);
    ws.parse_errors.is_empty()
}

#[test]
fn run_source_typed_args_single_line_body() {
    let got = build_run_source(
        "record<x: int>",
        "record<out: int>",
        &obj(r#"{"x":5}"#),
        "{ out: ($args.x + 1) }",
        "nonce123",
    );
    let expected = r#"do {
    $env.NONCE = "nonce123"
    def __run [args: record<x: int>]: nothing -> record<out: int> {
        { out: ($args.x + 1) }
    }
    __run {x: 5}
}
"#;
    assert_eq!(got, expected);
    assert!(
        parses_clean(&got),
        "rendered run source must parse clean:\n{got}"
    );
}

#[test]
fn run_source_void_args() {
    let got = build_run_source(
        "nothing",
        "record<out: int>",
        &obj("{}"),
        "{ out: 0 }",
        "nonce123",
    );
    let expected = r#"do {
    $env.NONCE = "nonce123"
    def __run [args: nothing]: nothing -> record<out: int> {
        { out: 0 }
    }
    __run null
}
"#;
    assert_eq!(got, expected);
    assert!(
        parses_clean(&got),
        "void run source must parse clean:\n{got}"
    );
}

#[test]
fn run_source_multi_line_body() {
    let got = build_run_source(
        "record<x: int>",
        "record<out: int>",
        &obj(r#"{"x":3}"#),
        "let y = ($args.x * 2)\n{ out: $y }",
        "nonce123",
    );
    let expected = r#"do {
    $env.NONCE = "nonce123"
    def __run [args: record<x: int>]: nothing -> record<out: int> {
        let y = ($args.x * 2)
{ out: $y }
    }
    __run {x: 3}
}
"#;
    assert_eq!(got, expected);
    assert!(
        parses_clean(&got),
        "multi-line run source must parse clean:\n{got}"
    );
}

#[test]
fn call_source_typed_args() {
    // The call target's OWN module is overlaid (nu 0.114 no longer implicitly
    // imports submodules, so the library root binds nothing to traverse), aliased
    // so the drive never depends on the target's name, and --prefix'd so the
    // target's helper exports don't flatten bare into the eval scope.
    let got = build_call_source("sourcetrait/calc", "math", "double", &obj(r#"{"x":6}"#), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/sourcetrait/calc/math/double as __call\n__call {x: 6}\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_nested_module_path() {
    // A slash-separated module_path stays slash-separated: it is a path segment
    // of the target's store coordinate, not a drive chain.
    let got = build_call_source("sourcetrait/calc", "math/trig", "sin", &obj(r#"{"x":1}"#), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/sourcetrait/calc/math/trig/sin as __call\n__call {x: 1}\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_void_args() {
    // Void / no-arg main: empty args bind the bare `null` literal so the
    // `nothing` positional typechecks (a `{}` record would not).
    // Non-`sourcetrait` author proves the compound library is threaded through.
    let got = build_call_source("acme/util", "net", "ping", &obj("{}"), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/acme/util/net/ping as __call\n__call null\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_keyword_named_target_is_aliased() {
    // The reason the alias exists. `run` became a nushell KEYWORD in 0.114 and
    // cannot be shadowed by a module under any import form -- a bare drive would
    // hit the builtin. The synthesized alias sidesteps the namespace entirely, so
    // a call-target may be named anything (we have `emptwo:almost/rpg:run`).
    let got = build_call_source("sourcetrait/emptwo", "almost/rpg", "run", &obj(r#"{"x":6}"#), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/sourcetrait/emptwo/almost/rpg/run as __call\n__call {x: 6}\n";
    assert_eq!(got, expected);
    assert!(
        !expected.contains("\nrun "),
        "the drive must never be the target's own name",
    );
}

#[test]
fn interact_source_typed_args() {
    let got = build_interact_source(
        "record<x: int>",
        "record<out: int>",
        &obj(r#"{"x":5}"#),
        "{ out: ($args.x + 1) }",
        "nonce123",
    );
    let expected = "(\n    def --env __interact [args: record<x: int>]: nothing -> record<out: int> {\n        $env.NONCE = \"nonce123\"\n        { out: ($args.x + 1) }\n    } ;\n    __interact {x: 5}\n)\n";
    assert_eq!(got, expected);
    assert!(
        parses_clean(&got),
        "rendered interact source must parse clean:\n{got}"
    );
}

#[test]
fn interact_source_void_args() {
    let got = build_interact_source(
        "nothing",
        "record<out: int>",
        &obj("{}"),
        "{ out: 0 }",
        "nonce123",
    );
    let expected = "(\n    def --env __interact [args: nothing]: nothing -> record<out: int> {\n        $env.NONCE = \"nonce123\"\n        { out: 0 }\n    } ;\n    __interact null\n)\n";
    assert_eq!(got, expected);
    assert!(
        parses_clean(&got),
        "void interact source must parse clean:\n{got}"
    );
}
