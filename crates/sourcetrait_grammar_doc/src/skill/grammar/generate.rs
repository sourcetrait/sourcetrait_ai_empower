use crate::*;

pub(crate) fn generate_grammar_skill(
    skills_dir: &Path
) ->DocResult<nu::Value> {
    const SPAN: nu::Span = nu::Span::unknown();
    let skill_dir = skills_dir.join("grammar");
    let skill_file = skill_dir.join("SKILL.md");
    let record = nu::record! {
        "skill" => nu::record! {
            "dir" => nu::Value::string(skill_dir.to_str().expect("valid"), SPAN),
            "files" => nu::Value::list(
                vec![
                    nu::record! {
                        "name" => nu::Value::string("SKILL.md", SPAN),
                        "size" => nu::Value::filesize(nu::Filesize::new(32000), SPAN)
                    }.into_value(SPAN),
                    nu::record! {
                        "name" => nu::Value::string("ANCHORED.md", SPAN),
                        "size" => nu::Value::filesize(nu::Filesize::new(28000), SPAN)
                    }.into_value(SPAN),
                ], SPAN),
        }.into_value(SPAN),
    };
    
    Ok(record.into_value(SPAN))
}