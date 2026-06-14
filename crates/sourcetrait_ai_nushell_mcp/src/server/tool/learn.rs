use crate::*;

/// What: agent-facing parameters for `learn()`. Carries the harness
/// directory under which the generated `skills/nu/SKILL.md` is written.
///
/// Why: learn() ships the nushell_mcp operating skill as a real file
/// on disk (Read-friendly, compaction-resilient) rather than returning
/// ~20KB of doc through the tool result. `harness_dir` is the generic
/// Claude-Code harness root; learn() composes
/// `<harness_dir>/skills/nu/SKILL.md` from it (the skill name "nu" is
/// the MCP's own const). Any agent or harness using the MCP can call it.
///
/// Where: extracted in `NuSh::learn`; passed to
/// `learn::generate_skill`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct LearnParams {
    /// Claude-Code harness root. learn() writes the generated skill to
    /// `<harness_dir>/skills/nu/SKILL.md`, creating the `skills/nu/`
    /// parents as needed.
    pub harness_dir: String,
}

/// What: success envelope for `learn()`. Reports the path written, its
/// byte length, and the server version stamped into the skill.
///
/// Why: writing to disk leaves one line in agent context (path +
/// bytes) instead of the full skill body; the agent Reads the path on
/// demand. `version` lets the caller compare against `info().version`
/// to decide whether to regenerate.
///
/// Where: returned from `NuSh::learn` via `envelope_to_structured`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct LearnEnvelope {
    pub written_path: String,
    pub bytes: u64,
    pub version: String,
}

/// The embedded liquid template for the `/nu` skill. Source-of-truth
/// for the skill body lives here (the harness `skills/nu/SKILL.md` is
/// a generated artifact). Only `{{ version }}` / `{{ nu_version }}` in
/// the stamp line interpolate; the rest is verbatim.
const NU_SKILL_TEMPLATE: &str =
    include_str!("../../../assets/templates/nu_skill.md.liquid");

/// Skill name in the Claude-Code layout `<harness_dir>/skills/<name>/SKILL.md`.
const SKILL_NAME: &str = "nu";

/// What: the liquid render context. Carries the two live values seeded
/// into the skill stamp.
///
/// Why: minimal seeding now (the_user 2026-06-14) -- one live
/// interpolation proves the pipeline; later expansion adds fields +
/// `{{ }}` placeholders.
///
/// Where: built in `generate_skill`, passed to `liquid::to_object`.
#[derive(ser::Serialize)]
struct LearnContext {
    version: String,
    nu_version: String,
}

/// What: renders the embedded `/nu` skill template with the live
/// version values and writes it to `<harness_dir>/skills/nu/SKILL.md`,
/// returning the written path + byte length.
///
/// Why: the learn() tool's core. Liquid parser-build / parse / context
/// / render failures map to `Error::Internal` with a `learn::*` phase
/// (the_user 2026-06-14) so the agent gets a typed envelope; the
/// crate's `From<io::Error>` covers the create_dir_all + write `?`
/// paths under phase "io".
///
/// Where: called by `NuSh::learn`.
pub(crate) fn generate_skill(
    harness_dir: &std::path::Path,
    version: &str,
    nu_version: &str,
) -> Result<(PathBuf, u64), Error> {
    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| Error::Internal {
            phase: "learn::parser".to_string(),
            reason: e.to_string(),
        })?;
    let template = parser.parse(NU_SKILL_TEMPLATE).map_err(|e| Error::Internal {
        phase: "learn::parse".to_string(),
        reason: e.to_string(),
    })?;
    let globals = liquid::to_object(&LearnContext {
        version: version.to_string(),
        nu_version: nu_version.to_string(),
    })
    .map_err(|e| Error::Internal {
        phase: "learn::context".to_string(),
        reason: e.to_string(),
    })?;
    let rendered = template.render(&globals).map_err(|e| Error::Internal {
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
        description = "Generate the nushell_mcp operating skill (the /nu SKILL.md: tool surface, schemas, lint, library-authoring conventions, Nu Nuances) and write it to <harness_dir>/skills/nu/SKILL.md, version-stamped to this server. Read the returned path on demand; regenerate when info().version differs from the stamp.",
        output_schema = mcp::schema_for_type::<LearnEnvelope>()
    )]
    async fn learn(
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
