use crate::*;

/// Parameters for `learn()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct LearnParams {
    /// Harness root; the skill lands at `<harness_dir>/skills/nu/SKILL.md`.
    pub harness_dir: String,
}

/// Success result of `learn()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct LearnEnvelope {
    pub written_path: String,
    pub bytes: u64,
    pub version: String,
}

const NU_SKILL_TEMPLATE: &str = include_str!("../../../assets/templates/nu_skill.md.liquid");

const SKILL_NAME: &str = "nu";

#[derive(ser::Serialize)]
struct LearnContext {
    version: String,
    nu_version: String,
}

pub(crate) fn generate_skill(
    harness_dir: &std::path::Path,
    version: &str,
    nu_version: &str,
) -> Result<(PathBuf, u64), GrammarMcpError> {
    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| GrammarMcpError::Internal {
            phase: "learn::parser".to_string(),
            reason: e.to_string(),
        })?;
    let template = parser
        .parse(NU_SKILL_TEMPLATE)
        .map_err(|e| GrammarMcpError::Internal {
            phase: "learn::parse".to_string(),
            reason: e.to_string(),
        })?;
    let globals = liquid::to_object(&LearnContext {
        version: version.to_string(),
        nu_version: nu_version.to_string(),
    })
    .map_err(|e| GrammarMcpError::Internal {
        phase: "learn::context".to_string(),
        reason: e.to_string(),
    })?;
    let rendered = template.render(&globals).map_err(|e| GrammarMcpError::Internal {
        phase: "learn::render".to_string(),
        reason: e.to_string(),
    })?;
    let dir = harness_dir.join("skills").join(SKILL_NAME);
    fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    fs::write(&path, rendered.as_bytes())?;
    Ok((path, rendered.len() as u64))
}

#[mcp::tool_router(router = learn_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Generate the latest `/nu` SKILL.md.",
        output_schema = mcp::schema_for_type::<LearnEnvelope>()
    )]
    pub(crate) async fn learn(
        &self,
        mcp::Parameters(p): mcp::Parameters<LearnParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let version = env!("CARGO_PKG_VERSION");
        match generate_skill(
            std::path::Path::new(&p.harness_dir),
            version,
            env!("NU_VERSION"),
        ) {
            Ok((path, bytes)) => envelope_to_structured(&LearnEnvelope {
                written_path: path.display().to_string(),
                bytes,
                version: version.to_string(),
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
