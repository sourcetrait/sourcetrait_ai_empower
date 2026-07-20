use crate::*;

/// Validate a fill record against a `soak.schema.nutype` type expression.
///
/// The schema is a nushell type (e.g. `record<iter: string>`), parsed via
/// nu-parser into a `nu_protocol::Type` and checked with `Value::is_subtype_of`
/// (open records -- exactly `let fill: <schema> = $fill`). nu-parser's
/// `parse_type` is not public, so the type is recovered by parsing a typed
/// closure `{|fill: <schema>| null}` and reading the parameter's type back.
///
/// A present schema that fails to parse, or that resolves to `any` (a literal
/// `any` or the parser's error fallback), is itself an error: the file's
/// existence means real validation is intended.
pub(crate) fn validate_fill(fill: &nu::Value, schema: &str) -> NuPluginGrimoireResult<()> {
    let engine_state = nu::EngineState::new();
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
    let snippet = format!("{{|fill: {schema}| null}}");
    let block = nu::parse(&mut working_set, None, snippet.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
        let detail = working_set
            .parse_errors
            .iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(nu_plugin_error(format!(
            "invalid soak.schema.nutype `{schema}`: {detail}"
        )));
    }
    let Some(ty) = closure_param_type(&working_set, &block) else {
        return Err(nu_plugin_error(format!(
            "could not read a type from soak.schema.nutype `{schema}`"
        )));
    };
    if matches!(ty, nu::Type::Any) {
        return Err(nu_plugin_error(format!(
            "soak.schema.nutype `{schema}` resolves to `any`, which validates nothing; \
             use a concrete type or remove the file"
        )));
    }
    if !fill.is_subtype_of(&ty) {
        return Err(nu_plugin_error(format!(
            "fill does not match soak.schema.nutype `{schema}` (expected {ty})"
        )));
    }
    Ok(())
}

/// Pull `to_type()` of the single required positional from a parsed
/// `{|fill: <schema>| ...}` closure block.
fn closure_param_type(
    working_set: &nu::StateWorkingSet,
    block: &nu::Block,
) -> Option<nu::Type> {
    let element = block.pipelines.first()?.elements.first()?;
    let nu::Expr::Closure(block_id) = &element.expr.expr else {
        return None;
    };
    let closure = working_set.get_block(*block_id);
    let positional = closure.signature.required_positional.first()?;
    Some(positional.shape.to_type())
}
