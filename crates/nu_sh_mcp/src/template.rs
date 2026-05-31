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
    out.push_str(&p.closure_body);
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
