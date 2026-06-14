use crate::*;

/// What: agent-facing parameters for `info`. Empty -- info takes no
/// input.
///
/// Why: `info()` surfaces static server state for agent
/// introspection; nothing per-call to parameterize.
///
/// Where: extracted in `NuSh::info`; the handler builds the envelope
/// from `env!` macros (name + version + nu_version) and a host-side
/// plugin enumeration.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InfoParams {}

/// What: agent-facing success envelope for `info`. Carries the
/// crate name + version, the embedded nushell engine version
/// captured at build time, the list of plugin names + versions
/// visible to the worker's registry, and the registered-library
/// hierarchy (library -> module -> function nodes with the call
/// schema typedefs).
///
/// Why: lifts handshake-only `serverInfo` to the tool surface so
/// agents can read it via `tools/call` instead of relying on the
/// rmcp client to relay handshake metadata. Plugin list is
/// enumerated from the same `plugin.msgpackz` registry the worker
/// loads from at startup; the library hierarchy is enumerated live
/// from the canonical repo so an agent (or subagent) discovers
/// call() targets without stale skill docs.
///
/// Where: returned from `NuSh::info` wrapped in a `CallToolResult`
/// whose `structured_content` carries the serialized envelope.
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
        description = "Name, version, nu version, nu plugins, and the registered library/module/function hierarchy with call schemas.",
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
