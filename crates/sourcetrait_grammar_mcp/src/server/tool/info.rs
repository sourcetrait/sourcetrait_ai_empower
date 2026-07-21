use crate::*;

/// Parameters for `info()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InfoParams {
    /// Purview ids to render as if they were in view. EMPTY (the default) means
    /// the CURRENT purview. Supplying ids does not change what is in view.
    #[serde(default)]
    pub purviews: Vec<String>,
}

/// Success result of `info()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InfoEnvelope {
    pub name: String,
    pub version: String,
    pub nu_version: String,
    /// The state-store coordinate this server was configured with
    /// (`--id` / `--namespace`) -- lets an agent self-confirm which
    /// store it is on.
    pub id: String,
    pub namespace: String,
    /// This host process's id, stable for its lifetime. A change across two calls
    /// means the server was restarted.
    pub mcp_nom: String,
    /// The agent working directory this server was configured with
    /// (`--workdir`), exported to eval bodies as $env.EQUIP_WORK_DIR.
    pub work_dir: String,
    pub plugins: Vec<crate::plugins::PluginInfo>,
    /// The callable surface WITHIN the reported purview, as one indented block
    /// where the indentation is the hierarchy: depth 0 an author, depth 1 a
    /// library, deeper a module unless it carries the two signature groups,
    /// which makes it a call.
    pub signatures: String,
    /// The purview `signatures` was rendered for: each id beside the selectors
    /// it resolves to. Last, because it is the frame around the block rather
    /// than part of it.
    pub purview: Vec<PurviewView>,
}

#[mcp::tool_router(router = info_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Versions, plugins, and every library's callable signatures.",
        output_schema = mcp::schema_for_type::<InfoEnvelope>()
    )]
    pub(crate) async fn info(
        &self,
        mcp::Parameters(p): mcp::Parameters<InfoParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // A purviews file that exists but will not decode is an ERROR, never a
        // silently empty (and therefore silently total) view - the same rule the
        // library index already holds itself to.
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        // EMPTY means CURRENT. Ids given explicitly are the subagent blinders:
        // they render as if those were in view without changing what is.
        let ids = if p.purviews.is_empty() {
            self.current_purview.ids()
        } else {
            p.purviews.clone()
        };
        let selectors = parse_selectors(&resolve_selectors(&ids, rows.as_ref()));
        envelope_to_structured(&InfoEnvelope {
            name: lib_grammar::consts::GRAMMAR.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            nu_version: env!("NU_VERSION").to_string(),
            id: config().id.clone(),
            namespace: config().namespace.clone(),
            mcp_nom: self.mcp_nom.to_string(),
            work_dir: config().work_dir.display().to_string(),
            plugins: list_registered_plugins(),
            signatures: render_signatures_within(&self.library_locks, &selectors).await,
            purview: purview_views(&ids, rows.as_ref()),
        })
    }
}
