use std::os::unix::fs::MetadataExt;

use crate::*;

static ASSETS: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets/reign/human/skill/grammar");

type Partials = liquid::partials::EagerCompiler<liquid::partials::InMemorySource>;

pub(crate) fn generate_grammar_skill(
    skills_dir: &Path
) ->DocResult<nu::Value> {
    let mut partials = Partials::empty();
    for partial in ASSETS.find("section/**/*.liquid").unwrap() {
        if let Some(file) = partial.as_file() {
            partials.add(file.path().to_str().unwrap(), file.contents_utf8().unwrap());
        }
    }
    
    let template = liquid::ParserBuilder::with_stdlib()
        .partials(partials)
        .build().unwrap()
        .parse(ASSETS.get_file("SKILL.md.liquid").unwrap().contents_utf8().unwrap())
        .unwrap();

    let skill_dir = skills_dir.join("grammar");
    let skill_file = skill_dir.join("SKILL.md");

    if !skill_dir.exists() {
        std::fs::create_dir_all(&skill_dir).unwrap();
    }

    let data = liquid::object!({});
    let out = template.render(&data).unwrap();
    std::fs::write(&skill_file, &out).unwrap();
    let skill_md_size = std::fs::metadata(skill_file).unwrap().len();
    
    const SPAN: nu::Span = nu::Span::unknown();
    let record = nu::record! {
        "skill" => nu::record! {
            "name" => nu::Value::string("grammar", SPAN),
            "dir" => nu::Value::string(skill_dir.to_str().expect("valid"), SPAN),
            "files" => nu::Value::list(
                vec![
                    nu::record! {
                        "name" => nu::Value::string("SKILL.md", SPAN),
                        "size" => nu::Value::filesize(nu::Filesize::new(skill_md_size as i64), SPAN)
                    }.into_value(SPAN),
                    /*nu::record! {
                        "name" => nu::Value::string("ANCHORED.md", SPAN),
                        "size" => nu::Value::filesize(nu::Filesize::new(28000), SPAN)
                    }.into_value(SPAN),*/
                ], SPAN),
        }.into_value(SPAN),
    };
    
    Ok(record.into_value(SPAN))
}