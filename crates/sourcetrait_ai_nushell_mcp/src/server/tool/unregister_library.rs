use crate::*;

/// What: agent-facing parameters for `unregister_library`. Just the
/// library name; the lock + meta + subtree are all keyed by it.
///
/// Why: unregister drops the library from the MCP repo + lock
/// registry. The client mirror is intentionally NOT touched per the
/// design call -- the client owns its copies after unregister.
///
/// Where: extracted in `NuSh::unregister_library`; passed to
/// `library::unregister_library_impl`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UnregisterLibraryParams {
    pub name: String,
}

#[mcp::tool_router(router = unregister_library_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Drop a library and all its functions from the MCP-managed canonical repo. Does not touch the agent's local mirror.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn unregister_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<UnregisterLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.unregister(&p.name).await {
            Ok(l) => l,
            Err(_) => return Ok(error_to_call_result(
                Error::LibraryNotRegistered { library: p.name.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match unregister_library_impl(&p.name) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
