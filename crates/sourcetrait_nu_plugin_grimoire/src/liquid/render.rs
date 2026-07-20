use crate::*;

/// Render a Liquid template string with a nu record of fill values.
///
/// The record becomes the Liquid global object (recursively converted) and the
/// full stdlib parser renders the template. Build / parse / render failures
/// surface as plugin errors carrying the liquid message.
pub(crate) fn render_template(
    template: &str,
    fill: &nu::Record,
) -> NuPluginGrimoireResult<String> {
    let globals = object_from_record(fill);
    let parser = match ::liquid::ParserBuilder::with_stdlib().build() {
        Ok(parser) => parser,
        Err(error) => {
            return Err(nu_plugin_error(format!("liquid parser build failed: {error}")));
        }
    };
    let template = match parser.parse(template) {
        Ok(template) => template,
        Err(error) => {
            return Err(nu_plugin_error(format!("liquid template parse failed: {error}")));
        }
    };
    match template.render(&globals) {
        Ok(rendered) => Ok(rendered),
        Err(error) => Err(nu_plugin_error(format!("liquid render failed: {error}"))),
    }
}

/// A nu record -> Liquid object (each value recursively converted).
fn object_from_record(record: &nu::Record) -> ::liquid::Object {
    let mut object = ::liquid::Object::new();
    for (key, value) in record.iter() {
        object.insert(key.clone().into(), value_to_liquid(value));
    }
    object
}

/// A nu value -> Liquid value. Scalars map directly; lists -> arrays, records ->
/// objects, null -> Nil. Any other nu type degrades to its string form -- a fill
/// is config-shaped data, so the catch-all is a safety net, not a hot path.
fn value_to_liquid(value: &nu::Value) -> ::liquid::model::Value {
    match value {
        nu::Value::Bool { val, .. } => ::liquid::model::Value::scalar(*val),
        nu::Value::Int { val, .. } => ::liquid::model::Value::scalar(*val),
        nu::Value::Float { val, .. } => ::liquid::model::Value::scalar(*val),
        nu::Value::String { val, .. } => ::liquid::model::Value::scalar(val.clone()),
        nu::Value::Glob { val, .. } => ::liquid::model::Value::scalar(val.clone()),
        nu::Value::Nothing { .. } => ::liquid::model::Value::Nil,
        nu::Value::List { vals, .. } => {
            ::liquid::model::Value::array(vals.iter().map(value_to_liquid))
        }
        nu::Value::Record { val, .. } => {
            ::liquid::model::Value::Object(object_from_record(val))
        }
        other => ::liquid::model::Value::scalar(other.coerce_string().unwrap_or_default()),
    }
}
