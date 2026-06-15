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
    pub plugins: Vec<crate::plugins::PluginInfo>,
    pub libraries: Vec<LibraryInfo>,
}

#[mcp::tool_router(router = info_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Versions, plugins, and libraries summary.",
        output_schema = mcp::schema_for_type::<InfoEnvelope>()
    )]
    async fn info(
        &self,
        mcp::Parameters(_p): mcp::Parameters<InfoParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        envelope_to_structured(&InfoEnvelope {
            name: build_target().name().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            nu_version: env!("NU_VERSION").to_string(),
            plugins: list_registered_plugins(),
            libraries: enumerate_libraries(&self.library_locks).await,
        })
    }
}
