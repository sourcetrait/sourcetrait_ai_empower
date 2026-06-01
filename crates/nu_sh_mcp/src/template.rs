use crate::*;

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
    // Serialize the args object as JSON. JSON object syntax is a valid
    // nushell record literal, so the same string serves both roles --
    // no decode step on the worker side.
    let args_json = json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string());
    out.push_str(&args_json);
    out.push_str(")\n");
    out.push_str("}\n");
    out
}

/// What: same shape as `build_run_source` but with NO outer
/// `do { ... }` wrapper. `__exec` and `__resolve` land at the top
/// level of the worker's persistent `engine_state` so they survive
/// into the next `interact()` call (re-defined each call, which
/// nushell allows).
///
/// Why: `interact()` advertises stateful semantics -- `merge_env`
/// after `eval_block` (in `worker::request_loop:: eval_source`'s
/// Stateful branch) propagates env mutations and `cd` from each call
/// to the persistent engine state.
///
/// Where: called by `server::tool::NuSh::interact` to produce the
/// source string that gets shipped to the stateful worker process.
/// Pairs with `Mode::Stateful` in `worker::request_loop::
/// eval_source`.
pub(crate) fn build_interact_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("def __exec [args: record<");
    out.push_str(&p.args_schema);
    out.push_str(">] {\n");
    out.push_str(&p.body);
    out.push_str("\n}\n");
    out.push_str("def __resolve [result: record<");
    out.push_str(&p.result_schema);
    out.push_str(">] { $result }\n");
    out.push_str("__resolve (__exec ");
    let args_json = json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string());
    out.push_str(&args_json);
    out.push_str(")\n");
    out
}
