use crate::*;

pub struct MdFind;

impl SimplePluginCommand for MdFind {
    type Plugin = EmpowerPlugin;

    fn name(&self) -> &str {
        "peek md find"
    }

    fn description(&self) -> &str {
        "Find regex matches in a markdown file; return `[offset, length]` pairs."
    }

    fn signature(&self) -> Signature {
        Signature::build("peek md find")
            .required(
                "pattern",
                SyntaxShape::String,
                "Regex pattern (multiline mode default).",
            )
            .required(
                "path",
                SyntaxShape::Filepath,
                "Markdown file path.",
            )
            .category(Category::FileSystem)
    }

    fn examples(&self) -> Vec<Example<'_>> {
        vec![Example {
            example: r#"peek md find '^#### ' SKILL.md"#,
            description: "Find all H4 heading offsets in SKILL.md.",
            result: None,
        }]
    }

    fn run(
        &self,
        _plugin: &EmpowerPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        _input: &Value,
    ) -> Result<Value, LabeledError> {
        let pattern: String = call.req(0)?;
        let path: PathBuf = call.req(1)?;
        let matches = markdown::find(&path, &pattern).map_err(|e| {
            LabeledError::new(e.to_string()).with_label(e.to_string(), call.head)
        })?;
        Ok(Value::list(
            matches
                .into_iter()
                .map(|(offset, length)| {
                    Value::list(
                        vec![
                            Value::int(offset as i64, call.head),
                            Value::int(length as i64, call.head),
                        ],
                        call.head,
                    )
                })
                .collect(),
            call.head,
        ))
    }
}
