use crate::*;

pub(crate) fn build_run_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("do {\n");
    for helper in &p.functions {
        out.push_str("    def ");
        out.push_str(&helper.name);
        out.push_str(" [args: record<");
        out.push_str(&helper.args_schema);
        out.push_str(">] {\n");
        out.push_str(&helper.body);
        out.push_str("\n    }\n");
    }
    out.push_str("    def __exec [args: record<");
    out.push_str(&p.args_schema);
    out.push_str(">] {\n");
    out.push_str(&p.closure);
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

/// Same shape as `build_run_source` but no outer `do { ... }` wrapper:
/// the helper defs, `__exec`, and `__resolve` land at the top level of
/// the worker's persistent `engine_state` so they survive into the next
/// interact() call (re-defined each call, which nushell allows).
/// Pairs with `Mode::Stateful` in `worker::request_loop::eval_source`,
/// which evals against `warm_base.engine_state` directly and calls
/// `merge_env` after each request so env mutations and `cd` persist.
pub(crate) fn build_interact_source(p: &RunParams) -> String {
    let mut out = String::with_capacity(256);
    for helper in &p.functions {
        out.push_str("def ");
        out.push_str(&helper.name);
        out.push_str(" [args: record<");
        out.push_str(&helper.args_schema);
        out.push_str(">] {\n");
        out.push_str(&helper.body);
        out.push_str("\n}\n");
    }
    out.push_str("def __exec [args: record<");
    out.push_str(&p.args_schema);
    out.push_str(">] {\n");
    out.push_str(&p.closure);
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
