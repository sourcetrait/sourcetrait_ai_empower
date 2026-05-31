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
    out.push_str(&p.args.to_string());
    out.push_str(")\n");
    out.push_str("}\n");
    out
}
