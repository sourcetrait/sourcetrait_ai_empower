use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = GrimoirePlugin;

    fn name(&self) -> &str {
        "from grimoire liquid"
    }

    fn description(&self) -> &str {
        "Render a Liquid template (pipeline string) with a record of fill values."
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("from grimoire liquid")
            .required(
                "fill",
                nu::SyntaxShape::Record(vec![].into()),
                "Record of values interpolated into the template.",
            )
            .input_output_types(vec![(nu::Type::String, nu::Type::String)])
            .category(nu::Category::Formats)
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        vec![nu::Example {
            example: r#"'hello {{ name }}' | from grimoire liquid { name: "world" }"#,
            description: "Render an inline template against a fill record.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &GrimoirePlugin,
        _engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        let fill: nu::Value = call.req(0)?;
        let fill = fill
            .as_record()
            .map_err(|error| labeled_error(error, call.head))?;
        let template = input
            .as_str()
            .map_err(|error| labeled_error(error, call.head))?;
        let rendered =
            render_template(template, fill).map_err(|error| labeled_error(error, call.head))?;
        Ok(nu::Value::string(rendered, call.head))
    }
}
