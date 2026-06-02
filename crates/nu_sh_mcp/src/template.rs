use crate::*;

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
/// Where: called by `build_run_source` (substituting into the
/// `__exec ARGS` call site) and `build_interact_source` (substituting
/// into the `let args: record<...> = ARGS` typed-let RHS) once per
/// template emission.
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
            let items: Vec<nu::Value> =
                arr.iter().map(json_value_to_nu_value).collect();
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

/// What: builds the nushell source the stateless worker will eval for
/// a `run()` call. Emits a `do { ... }` block containing an `__exec`
/// def carrying the agent's body, a `__resolve` def that gates the
/// return value against `result_schema`, and a final
/// `__resolve (__exec ARGS_JSON)` invocation where ARGS_JSON is the
/// agent's args serialized as a record literal.
///
/// Why: the outer `do { ... }` is load-bearing -- nushell's
/// Arc::make_mut COW on blocks means defs declared inside a do-block
/// don't persist in the worker's EngineState across calls, which is
/// exactly the stateless contract `run()` advertises. The
/// `__exec`/`__resolve` two-def template encodes typed positional
/// binding at the agent-facing boundary so both args mismatches and
/// result mismatches surface as nu errors with precise spans.
///
/// Where: called by `server::tool::NuSh::run` (and by `rerun`, which
/// reconstructs a RunParams from cache and reuses the same builder)
/// to produce the source string that gets shipped through
/// `WorkerHandle::send_request` to the stateless worker process.
pub(crate) fn build_run_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("do {\n");
    out.push_str("    def __exec [args: record<");
    out.push_str(&p.args_schema);
    out.push_str(">] {\n");
    out.push_str(&p.body);
    out.push_str("\n    }\n");
    out.push_str("    def __resolve [result: record<");
    out.push_str(&p.result_schema);
    out.push_str(">] { $result }\n");
    out.push_str("    __resolve (__exec ");
    let args_nuon = args_to_nuon(&p.args);
    out.push_str(&args_nuon);
    out.push_str(")\n");
    out.push_str("}\n");
    out
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
pub(crate) fn build_interact_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("def __validate_result [] {\n");
    out.push_str("    let r: record<");
    out.push_str(&p.result_schema);
    out.push_str("> = $in\n");
    out.push_str("    $r\n");
    out.push_str("}\n");
    out.push_str("let args: record<");
    out.push_str(&p.args_schema);
    out.push_str("> = ");
    let args_nuon = args_to_nuon(&p.args);
    out.push_str(&args_nuon);
    out.push_str("\n");
    out.push_str(&p.body);
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
