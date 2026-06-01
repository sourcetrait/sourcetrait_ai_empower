use crate::*;

/// What: builds the nushell source the stateless worker will eval for
/// a `run()` call. Shape is an outer `do { ... }` wrapping a
/// `__validate_result` def, a typed-let binding for `$args`, a
/// subexpression-wrapped body assigned to `$__out`, and a final
/// `__validate_result $__out` call that gates the return against
/// `result_schema`.
///
/// Why: the outer `do { ... }` is load-bearing because run() is
/// stateless -- the engine_state is cloned per call and the clone is
/// dropped on return, so nothing the body does (incl. env mutations
/// inside the subexpression) reaches the next call. The typed-let on
/// args fires nushell's parse-time literal check against the
/// agent-supplied args record (verified via probe P1 +
/// `mem:nushell-internals-typecheck-timing`); the
/// `__validate_result` def fires a runtime typed-positional check on
/// the body's return value (catchable by try/catch). The subexpression
/// wrapper around BODY captures the terminating expression as `$__out`
/// without scoping away top-level mutations (subexpressions inherit
/// the enclosing scope's env, verified via probe P2).
///
/// Where: called by `server::tool::NuSh::run` (and by `rerun`, which
/// reconstructs a RunParams from cache and reuses the same builder)
/// to produce the source string that gets shipped through
/// `WorkerHandle::send_request` to the stateless worker process.
pub(crate) fn build_run_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("do {\n");
    out.push_str("    def __validate_result [r: record<");
    out.push_str(&p.result_schema);
    out.push_str(">] { $r }\n");
    out.push_str("    let args: record<");
    out.push_str(&p.args_schema);
    out.push_str("> = ");
    // JSON object syntax is a valid nushell record literal, so the
    // same serialized string serves both roles -- no decode step on
    // the worker side. The typed-let on the receiver enforces the
    // args_schema at parse time against this literal.
    let args_json = json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string());
    out.push_str(&args_json);
    out.push_str("\n");
    // Wrap BODY in `do { ... }` inside the subexpression so multi-line
    // statements parse correctly. A bare `( ... )` subexpression treats
    // adjacent pipelines on separate lines as concatenated call args
    // when the first pipeline is a command call (verified via probe P7:
    // `(sleep 100ms\n{out: 0})` parses as `sleep 100ms {out: 0}` and
    // hits TypeMismatch(Duration, Record)). `(do { ... })` runs the
    // block as a closure invocation; multi-line works the same as a
    // function body. Env mutations inside the do-block are scoped to
    // its frame, which is fine for `run` because the outer do-wrap
    // already clones engine_state per call (Mode::Stateless).
    out.push_str("    let __out = (do {\n");
    out.push_str(&p.body);
    out.push_str("\n    })\n");
    out.push_str("    __validate_result $__out\n");
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
