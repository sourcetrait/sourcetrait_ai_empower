use crate::*;

pub(crate) struct Command;

impl nu::SimplePluginCommand for Command {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "empowered eye md find"
    }

    fn description(&self) -> &str {
        "Find regex matches in a markdown file; return `[offset, length]` pairs."
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("empowered eye md find")
            .required(
                "pattern",
                nu::SyntaxShape::String,
                "Regex pattern (multiline mode default).",
            )
            .required(
                "path",
                nu::SyntaxShape::Filepath,
                "Markdown file path.",
            )
            .category(nu::Category::FileSystem)
    }

    fn examples(&self) -> Vec<nu::Example<'_>> {
        vec![nu::Example {
            example: r#"empowered eye md find '^#### ' SKILL.md"#,
            description: "Find all H4 heading offsets in SKILL.md.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &EmpowerPlugin,
        _engine: &nu::EngineInterface,
        call: &nu::EvaluatedCall,
        _input: &nu::Value,
    ) -> Result<nu::Value, nu::LabeledError> {
        let pattern: String = call.req(0)?;
        let path: PathBuf = call.req(1)?;
        let matches = lib::md::find(&path, &pattern).map_err(|e| {
            nu::LabeledError::new(e.to_string()).with_label(e.to_string(), call.head)
        })?;
        Ok(nu::Value::list(
            matches
                .into_iter()
                .map(|(offset, length)| {
                    nu::Value::list(
                        vec![
                            nu::Value::int(offset as i64, call.head),
                            nu::Value::int(length as i64, call.head),
                        ],
                        call.head,
                    )
                })
                .collect(),
            call.head,
        ))
    }
}
