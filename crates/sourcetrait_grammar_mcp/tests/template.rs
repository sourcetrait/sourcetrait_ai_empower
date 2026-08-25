//! Template synthesis + parse-cleanliness. Integration (not unit): `parses_clean`
//! builds a full-shell `ParseEngine` (plugin-registry read off disk), so it rides
//! the resource caveat. Driven through the `guts` surface.

use serde_json::json;
use sourcetrait_common::datum;
use sourcetrait_grammar_mcp::guts::{
    build_call_source, build_interact_source, build_run_source, parses_clean,
};

#[test]
fn run_source_typed_args_single_line_body() {
    let nonce = datum::NonceGenerator::new().generate().into_pair();
    let nonce_str = nonce.as_str();
    
    let got = build_run_source(
        "record<x: int>",
        "record<out: int>",
        json!({"x": 5}),
        "{ out: ($args.x + 1) }",
        &nonce,
    );
    let expected = indoc::formatdoc! {r#"
            do {{
                $env.NONCE = "{nonce_str}"
                def __run [args: record<x: int>]: nothing -> record<out: int> {{
                    {{ out: ($args.x + 1) }}
                }}
                __run {{x: 5}}
            }}
        "#,
        nonce_str = nonce_str,
    };
    
    assert_eq!(got, expected);
    assert!(parses_clean(&got), "rendered run source must parse clean:\n{got}");
}

#[test]
fn run_source_void_args() {
    let nonce = datum::NonceGenerator::new().generate().into_pair();
    let nonce_str = nonce.as_str();
    
    let got = build_run_source(
        "nothing",
        "record<out: int>",
        json!({}),
        "{ out: 0 }",
        &nonce,
    );
    let expected = indoc::formatdoc! {r#"
            do {{
                $env.NONCE = "{nonce_str}"
                def __run [args: nothing]: nothing -> record<out: int> {{
                    {{ out: 0 }}
                }}
                __run null
            }}
        "#,
        nonce_str = nonce_str,
    };
    
    assert_eq!(got, expected);
    assert!(parses_clean(&got), "void run source must parse clean:\n{got}");
}

#[test]
fn run_source_multi_line_body() {
    let nonce = datum::NonceGenerator::new().generate().into_pair();
    let nonce_str = nonce.as_str();
    
    let got = build_run_source(
        "record<x: int>",
        "record<out: int>",
        json!({"x": 3}),
        "let y = ($args.x * 2)\n{ out: $y }",
        &nonce,
    );
    // The body is spliced VERBATIM: continuation lines land at column 0 (plain
    // format! interpolation), which is deliberate - re-indenting them would
    // corrupt a body carrying a multi-line string literal. formatdoc cannot
    // express this expectation (a column-0 line would zero its dedent).
    let expected = format!(
        "do {{\n    $env.NONCE = \"{nonce_str}\"\n    def __run [args: record<x: int>]: nothing -> record<out: int> {{\n        let y = ($args.x * 2)\n{{ out: $y }}\n    }}\n    __run {{x: 3}}\n}}\n",
    );
    assert_eq!(got, expected);
    assert!(parses_clean(&got), "multi-line run source must parse clean:\n{got}");
}

#[test]
fn call_source_typed_args() {
    let got = build_call_source("sourcetrait/calc", "math", "double", json!({"x": 6}), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/sourcetrait/calc/math/double as __call\n__call {x: 6}\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_nested_module_path() {
    let got = build_call_source("sourcetrait/calc", "math/trig", "sin", json!({"x": 1}), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/sourcetrait/calc/math/trig/sin as __call\n__call {x: 1}\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_void_args() {
    let got = build_call_source("acme/util", "net", "ping", json!({}), "nonce123");
    let expected = "$env.NONCE = \"nonce123\"\noverlay use --prefix rig/acme/util/net/ping as __call\n__call null\n";
    assert_eq!(got, expected);
}

#[test]
fn call_source_keyword_named_target_is_aliased() {
    let got = build_call_source("sourcetrait/emptwo", "almost/rpg", "run", json!({"x": 6}), "nonce123");
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
        json!({"x": 5}),
        "{ out: ($args.x + 1) }",
        "nonce123",
    );
    let expected = "(\n    def --env __interact [args: record<x: int>]: nothing -> record<out: int> {\n        $env.NONCE = \"nonce123\"\n        { out: ($args.x + 1) }\n    } ;\n    __interact {x: 5}\n)\n";
    assert_eq!(got, expected);
    assert!(parses_clean(&got), "rendered interact source must parse clean:\n{got}");
}

#[test]
fn interact_source_void_args() {
    let got = build_interact_source(
        "nothing",
        "record<out: int>",
        json!({}),
        "{ out: 0 }",
        "nonce123",
    );
    let expected = "(\n    def --env __interact [args: nothing]: nothing -> record<out: int> {\n        $env.NONCE = \"nonce123\"\n        { out: 0 }\n    } ;\n    __interact null\n)\n";
    assert_eq!(got, expected);
    assert!(parses_clean(&got), "void interact source must parse clean:\n{got}");
}
