use crate::*;

/// What: agent-facing parameters for `reimport_library`. Just the
/// library name; the original source path comes from the meta
/// sidecar.
///
/// Why: reimport re-reads from the path the agent originally
/// supplied to `import_library`, re-runs validation, replaces the
/// canonical copy. No new path parameter because mismatch with the
/// original would silently corrupt the registration.
///
/// Where: extracted in `NuSh::reimport_library`; passed to
/// `library::reimport_library_impl`. Errors if the library was
/// register-style instead of import-style.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ReimportLibraryParams {
    /// Library name. Must be already registered AND of kind=imported.
    pub name: String,
}

#[mcp::tool_router(router = reimport_library_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Re-import a library from the path it was originally imported from. Reads source_path from the library's metadata; re-runs strict validation; replaces the canonical copy with a fresh snapshot. Errors if the library was register_library-style (kind=registered) instead of import_library-style.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn reimport_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ReimportLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.lookup(&p.name).await {
            Some(l) => l,
            None => return Ok(error_to_call_result(
                Error::LibraryNotRegistered { library: p.name.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match reimport_library_impl(&p.name, &self.lint_engine) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
