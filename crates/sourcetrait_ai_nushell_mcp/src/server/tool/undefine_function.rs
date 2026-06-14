use crate::*;

/// What: agent-facing parameters for `undefine_function`. Mirrors
/// `DefineFunctionParams`'s coordinate fields without the body and
/// schemas.
///
/// Why: undefine is purely a coordinate lookup -- the cached body
/// is on disk and gets removed; the cascade is updated; the commit
/// records the operation. No need for body/schemas on the wire.
///
/// Where: extracted in `NuSh::undefine_function`; passed to
/// `library::undefine_function_impl`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UndefineFunctionParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
}

#[mcp::tool_router(router = undefine_function_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Remove a function from a registered library. Updates the `mod.nu` cascade, prunes any now-empty intermediate directories, mirrors the removal, and commits.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn undefine_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<UndefineFunctionParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.lookup(&p.library).await {
            Some(l) => l,
            None => return Ok(error_to_call_result(
                Error::LibraryNotRegistered { library: p.library.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match undefine_function_impl(&p.library, &p.module_path, &p.name) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
