use crate::*;

/// What: agent-facing parameters for `register_library`. Carries the
/// library name + the client mirror path.
///
/// Why: registering creates an EMPTY library namespace (subsequent
/// `define_function` calls populate it); the agent supplies the
/// mirror path now so the MCP can write to both locations
/// atomically from then on.
///
/// Where: extracted in `NuSh::register_library`; passed to
/// `library::register_library_impl`. The lock registry is the entry
/// point of record (atomic check-and-insert via `register`).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RegisterLibraryParams {
    /// Library name (top-level identifier). Becomes the directory name
    /// in the MCP repo under `$XDG_DATA_HOME/sourcetrait/nushell_mcp/libraries/`.
    pub name: String,
    /// Client-side path where the MCP mirrors the library's files.
    /// Created if absent. Subsequent define_function calls write here
    /// alongside the MCP's canonical copy.
    pub path: String,
}

#[mcp::tool_router(router = register_library_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Register an empty library namespace; subsequent define_function calls populate it on both the MCP-managed canonical repo and the agent's local mirror at `path`.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn register_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<RegisterLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.register(&p.name).await {
            Ok(l) => l,
            Err(_) => return Ok(error_to_call_result(
                Error::LibraryAlreadyRegistered { library: p.name.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match register_library_impl(&p.name, std::path::Path::new(&p.path)) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
