use crate::*;
use indoc::formatdoc;

/// What: convert the agent-supplied args (`mcp::JsonObject`) into a
/// NUON record-literal string ready for substitution into a nu
/// source template.
///
/// Why: NUON is nushell's native typed-data literal format and is
/// what the worker's parser will consume from the substituted source;
/// converting in Rust avoids round-tripping a JSON string through
/// nushell's own JSON parser at eval time AND aligns with the
/// `mem:sourcetrait-rust-conventions` "NUON is the user-visible wire
/// format for typed Values" convention. JSON happens to be a valid
/// nu record literal for JSON-shaped inputs, but ad-hoc reuse of the
/// JSON serialization is a worse fit for the convention and the
/// performance is worse (string parse work the parser doesn't need
/// to do).
///
/// Where: called (via `args_literal`) by `build_run_source` (the
/// gate-chain `__to_run` call site) and `build_interact_source` (the
/// `let args` RHS) once per template emission.
fn args_to_nuon(args: &mcp::JsonObject) -> String {
    let value = json_object_to_nu_value(args);
    let engine_state = nu::EngineState::new();
    nu::to_nuon(&engine_state, &value, nu::ToNuonConfig::default())
        .unwrap_or_else(|_| "{}".to_string())
}

/// Recursively convert a `serde_json::Map` (the `mcp::JsonObject`
/// shape) into a `nu_protocol::Value::Record`.
fn json_object_to_nu_value(map: &mcp::JsonObject) -> nu::Value {
    let span = nu::Span::unknown();
    let mut record = nu::Record::new();
    for (k, v) in map {
        record.insert(k.clone(), json_value_to_nu_value(v));
    }
    nu::Value::record(record, span)
}

/// Recursively convert a `serde_json::Value` into a
/// `nu_protocol::Value`. Numbers preserve the int/float distinction;
/// non-finite or out-of-i64-range numbers degrade to a string
/// preserving the original token (rare for agent-supplied JSON).
fn json_value_to_nu_value(v: &serde_json::Value) -> nu::Value {
    let span = nu::Span::unknown();
    match v {
        serde_json::Value::Null => nu::Value::nothing(span),
        serde_json::Value::Bool(b) => nu::Value::bool(*b, span),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                nu::Value::int(i, span)
            } else if let Some(f) = n.as_f64() {
                nu::Value::float(f, span)
            } else {
                nu::Value::string(n.to_string(), span)
            }
        }
        serde_json::Value::String(s) => nu::Value::string(s.clone(), span),
        serde_json::Value::Array(arr) => {
            let items: Vec<nu::Value> = arr.iter().map(json_value_to_nu_value).collect();
            nu::Value::list(items, span)
        }
        serde_json::Value::Object(map) => {
            let mut record = nu::Record::new();
            for (k, v) in map {
                record.insert(k.clone(), json_value_to_nu_value(v));
            }
            nu::Value::record(record, span)
        }
    }
}

/// What: the value substituted at the `__run` call site (and the
/// `build_interact_source` `let args` RHS). A void positional (`nothing`)
/// with empty args binds the bare `null`; otherwise the args record as
/// NUON. A void positional with NON-empty args emits the record NUON
/// against a `nothing` parameter and fails the typecheck -- strict void
/// (item 21).
///
/// Where: called by `build_run_source` + `build_interact_source` once per
/// template emission.
fn args_literal(args_type: &str, args: &mcp::JsonObject) -> String {
    if args_type == "nothing" && args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    }
}

/// What: builds the nushell source the stateless worker evals for a
/// `run()` call -- a `do { ... }` block with one infix-signatured def
/// `__run [args: A]: nothing -> R` carrying the agent body, invoked as
/// `__run ARGS_LITERAL`. ARGS_LITERAL is `args_literal` (NUON record, or
/// bare `null` for void args).
///
/// Why: the positional `[args: A]` runtime-enforces the args (missing /
/// wrong-typed rejected, even on any-typed values; verified). The
/// `: nothing -> R` output type documents the result and parse-checks a
/// statically-typed body's result (raises `OutputMismatch`); a dynamic /
/// any-typed result is unchecked and records are open (extra fields pass)
/// -- the accepted best-effort-result scope; strict extras are the
/// deferred record/table seal. The outer `do { ... }` is load-bearing:
/// defs inside it don't persist in the worker's EngineState across calls,
/// the stateless contract.
///
/// Where: called by `server::tool::NuSh::run` and by `rerun` (which
/// rebuilds RunParams from cache and reuses this builder); the rendered
/// source ships through `WorkerHandle::send_request` to a pool worker.
pub(crate) fn build_run_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
) -> String {
    let lit = args_literal(args_type, args);
    formatdoc!(
        r#"
        do {{
            def __run [args: {at}]: nothing -> {rt} {{
                {body}
            }}
            __run {lit}
        }}
    "#,
        at = args_type,
        rt = result_type,
        lit = lit,
        body = body
    )
}

/// What: builds the nushell source the stateless worker evals for a
/// `call()` -- `use PATH` imports the committed function file, then
/// `NAME LIT` invokes its `main` (NAME = the file stem; nushell runs a
/// module's `main` when the module name is called directly). LIT is the
/// args record as NUON, or bare `null` for empty / void args.
///
/// Why: the committed call-target is a single infix-signatured
/// `export def main [args: A]: nothing -> R` -- main's positional
/// runtime-enforces the args and its output type parse-checks a static
/// result, so the synthesis needs no resolve / call shim, just the
/// two-line drive of `main`. The path comes from `call_file_path`
/// (validated under the libraries dir); `use` resolves it at parse time.
///
/// Where: called by `server::tool::NuSh::call`; the rendered source ships
/// through a pool worker exactly like run() / rerun().
pub(crate) fn build_call_source(path: &str, name: &str, args: &mcp::JsonObject) -> String {
    let lit = if args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    };
    formatdoc!(
        r#"
        use {path}
        {name} {lit}
    "#,
        path = path,
        name = name,
        lit = lit,
    )
}

/// What: builds the nushell source the stateful worker will eval for
/// an `interact()` call. Emits a top-level `__validate_result` def, a
/// typed-let binding `$args` against the agent-supplied args
/// literal, the agent's body at top level, and a trailing pipeline
/// `| __validate_result | do {|x| hide __validate_result; $x}` that
/// runtime-typechecks the body's terminating value and scrubs the
/// validator def from `engine_state` after use.
///
/// Why: interact() advertises stateful semantics -- env mutations,
/// `cd`, and agent-defined top-level defs persist across calls via
/// `merge_env` + `merge_delta` (in `worker::request_loop::
/// eval_source`'s Stateful branch). To deliver that contract, BODY
/// must run at the top level of the eval'd source -- not inside a
/// function-body scope (which would hide mutations from the outer
/// Stack that merge_env reads). The typed-let on `$args` keeps
/// the args-schema parse-time check against the literal record
/// substitution. Result validation moves from a typed positional def
/// (impossible at top level without losing the multi-line body parse)
/// to a `def [...] { let r: record<RS> = $in; $r }` that consumes
/// the body's piped terminator and runtime-typechecks via the
/// typed-let on `$in`. The trailing `do {|x| hide ...; $x}` closure
/// cleans up the validator from engine_state so the agent's next
/// call sees a fresh namespace (verified cross-eval via probe P10).
/// `let args` lives on the Stack (per-eval) so it doesn't persist
/// (verified via probe P9), no cleanup needed.
///
/// Where: called by `server::tool::NuSh::interact` to produce the
/// source string that gets shipped to the stateful worker process.
/// Pairs with `Mode::Stateful` in `worker::request_loop::
/// eval_source`.
pub(crate) fn build_interact_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("def __validate_result [] {\n");
    out.push_str("    let r: ");
    out.push_str(result_type);
    out.push_str(" = $in\n");
    out.push_str("    $r\n");
    out.push_str("}\n");
    out.push_str("let args: ");
    out.push_str(args_type);
    out.push_str(" = ");
    out.push_str(&args_literal(args_type, args));
    out.push_str("\n");
    out.push_str(body);
    out.push_str("\n| __validate_result\n");
    // The closure scrubs the validator def from engine_state and
    // returns the validated value as the final pipeline output. Per
    // probe P10c (and the manual probe verifying $in semantics),
    // `hide <def>` inside a do-block propagates to engine_state via
    // merge_delta so the next interact() call sees a fresh namespace.
    // We bind the piped value via `$in` (not a positional `|x|`
    // because `value | do {|x| ...}` does NOT auto-bind the pipe
    // input to `x` -- it raises "Missing parameter: x").
    out.push_str("| do { let _v = $in; hide __validate_result; $_v }\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(s: &str) -> mcp::JsonObject {
        match serde_json::from_str::<json::Value>(s).unwrap() {
            json::Value::Object(m) => m,
            _ => panic!("not an object"),
        }
    }

    /// Parse the rendered source on the full-shell lint engine; a clean
    /// parse (no parse errors) is the gate the golden strings must clear.
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
        );
        let expected = r#"do {
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
        let got = build_run_source("nothing", "record<out: int>", &obj("{}"), "{ out: 0 }");
        let expected = r#"do {
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
        );
        let expected = r#"do {
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
        let got = build_call_source("/libs/calc/math/double.nu", "double", &obj(r#"{"x":6}"#));
        let expected = "use /libs/calc/math/double.nu\ndouble {x: 6}\n";
        assert_eq!(got, expected);
    }

    #[test]
    fn call_source_void_args() {
        // Void / no-arg main: empty args bind the bare `null` literal so the
        // `nothing` positional typechecks (a `{}` record would not).
        let got = build_call_source("/libs/util/ping.nu", "ping", &obj("{}"));
        let expected = "use /libs/util/ping.nu\nping null\n";
        assert_eq!(got, expected);
    }
}
