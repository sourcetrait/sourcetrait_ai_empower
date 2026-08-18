use crate::*;
use indoc::formatdoc;

const CALL_ALIAS: &str = "__call";

fn args_to_nuon(args: &mcp::JsonObject) -> String {
    let value = json_object_to_nu_value(args);
    let engine_state = nu::EngineState::new();
    nu::to_nuon(&engine_state, &value, nu::ToNuonConfig::default())
        .unwrap_or_else(|_| "{}".to_string())
}

fn json_object_to_nu_value(map: &mcp::JsonObject) -> nu::Value {
    let span = nu::Span::unknown();
    let mut record = nu::Record::new();
    for (k, v) in map {
        record.insert(k.clone(), json_value_to_nu_value(v));
    }
    nu::Value::record(record, span)
}

/// serde_json to nu: the one JSON-to-Value bridge in the crate.
pub(crate) fn json_value_to_nu_value(v: &serde_json::Value) -> nu::Value {
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

fn args_literal(args_type: &str, args: &mcp::JsonObject) -> String {
    if args_type == "nothing" && args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    }
}

pub(crate) fn build_run_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
    nonce: &datum::NoncePair,
) -> String {
    let nonce = nonce.str();
    let lit = args_literal(args_type, args);
    formatdoc!(
        r#"
        do {{
            $env.NONCE = "{nonce}"
            def __run [args: {at}]: nothing -> {rt} {{
                {body}
            }}
            __run {lit}
        }}
    "#,
        nonce = nonce,
        at = args_type,
        rt = result_type,
        lit = lit,
        body = body,
    )
}

pub(crate) fn build_call_source(
    rig: &str,
    module_path: &str,
    name: &str,
    args: &mcp::JsonObject,
    nonce: &str,
) -> String {
    let lit = if args.is_empty() {
        "null".to_string()
    } else {
        args_to_nuon(args)
    };
    let target = if module_path.is_empty() {
        format!("{rig}/{name}")
    } else {
        format!("{rig}/{module_path}/{name}")
    };
    formatdoc!(
        r#"
        $env.NONCE = "{nonce}"
        overlay use --prefix rig/{target} as {alias}
        {alias} {lit}
    "#,
        nonce = nonce,
        target = target,
        alias = CALL_ALIAS,
        lit = lit,
    )
}

pub(crate) fn build_interact_source(
    args_type: &str,
    result_type: &str,
    args: &mcp::JsonObject,
    body: &str,
    nonce: &str,
) -> String {
    let lit = args_literal(args_type, args);
    formatdoc!(
        r#"
        (
            def --env __interact [args: {at}]: nothing -> {rt} {{
                $env.NONCE = "{nonce}"
                {body}
            }} ;
            __interact {lit}
        )
    "#,
        nonce = nonce,
        at = args_type,
        rt = result_type,
        lit = lit,
        body = body,
    )
}
