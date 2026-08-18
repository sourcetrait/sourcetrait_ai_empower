use crate::*;

/// Parameters for `info()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InfoParams {
    /// Purview ids to render as if in view; EMPTY means the CURRENT purview.
    #[serde(default)]
    pub purviews: Vec<String>,
}

/// Success result of `info()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InfoEnvelope {
    pub name: String,
    pub version: String,
    pub nu_version: String,
    /// The `--id` this server was configured with.
    pub id: String,
    pub namespace: String,
    /// This host process's id, stable for its lifetime.
    pub mcp_nom: String,
    /// The agent working directory, exported as $env.EQUIP_WORK_DIR.
    pub work_dir: String,
    pub plugins: Vec<crate::plugins::PluginInfo>,
    /// The callable surface within the reported purview, as one block.
    pub signatures: String,
    /// The purview `signatures` was rendered for.
    pub purview: Vec<PurviewView>,
}

#[mcp::tool_router(router = info_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Versions, plugins, and every rig's callable signatures.",
        output_schema = mcp::schema_for_type::<InfoEnvelope>()
    )]
    pub(crate) async fn info(
        &self,
        mcp::Parameters(p): mcp::Parameters<InfoParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        let ids: Vec<String> = if p.purviews.is_empty() {
            self.current_purview.ids()
        } else {
            p.purviews
                .iter()
                .map(|id| purview_ref(id).unwrap_or(id).to_string())
                .collect()
        };
        let patterns = parse_patterns(&expand_values(
            &resolve_patterns(&ids, rows.as_ref()),
            rows.as_ref(),
        ));
        envelope_to_structured(&InfoEnvelope {
            name: lib_grammar::consts::GRAMMAR.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            nu_version: env!("NU_VERSION").to_string(),
            id: config().id.clone(),
            namespace: config().namespace.clone(),
            mcp_nom: self.mcp_nom.str().to_string(),
            work_dir: config().work_dir.display().to_string(),
            plugins: list_registered_plugins(),
            signatures: render_signatures_within(&self.rig_locks, &patterns).await,
            purview: purview_views(&ids, rows.as_ref()),
        })
    }
}
