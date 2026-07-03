use crate::*;

/// Parameters for `info()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InfoParams {}

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
    pub plugins: Vec<crate::plugins::PluginInfo>,
    pub libraries: Vec<LibraryInfo>,
}

#[mcp::tool_router(router = info_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Versions, plugins, and libraries summary.",
        output_schema = mcp::schema_for_type::<InfoEnvelope>()
    )]
    pub(crate) async fn info(
        &self,
        mcp::Parameters(_p): mcp::Parameters<InfoParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        envelope_to_structured(&InfoEnvelope {
            name: lib_empower::consts::NUSHELL_MCP.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            nu_version: env!("NU_VERSION").to_string(),
            id: config().id.clone(),
            namespace: config().namespace.clone(),
            plugins: list_registered_plugins(),
            libraries: enumerate_libraries(&self.library_locks).await,
        })
    }
}
