use crate::*;

/// What: agent-facing parameters for `import_library`. Carries the
/// library name plus the absolute path to a pre-authored library
/// source tree on the client.
///
/// Why: import is the agent-authored, MCP-validated path. The MCP
/// reads the source tree, runs the strict AST validator, then copies
/// it into the canonical repo. The recorded path is used by
/// `reimport_library` for re-snapshotting.
///
/// Where: extracted in `NuSh::import_library`; passed to
/// `library::import_library_impl`. The path is treated as immutable
/// metadata once written to `.nushell_mcp_meta.json`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ImportLibraryParams {
    /// Library name to register under. Must be unused.
    pub name: String,
    /// Absolute path to the directory holding the pre-authored library
    /// source. The MCP validates the tree, copies it into the canonical
    /// repo, and records this path for future `reimport_library` calls.
    pub path: String,
}

#[mcp::tool_router(router = import_library_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Import a pre-authored library from a client path into the MCP-managed canonical repo. Strict validation: each function file must have exactly `export def main [args: record<...>]` + `export def resolve [args: record<...>] { $args }`; each `mod.nu` may only re-export children. All violations are reported at once; no auto-fix.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn import_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ImportLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.register(&p.name).await {
            Ok(l) => l,
            Err(_) => return Ok(error_to_call_result(
                Error::LibraryAlreadyRegistered { library: p.name.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match import_library_impl(&p.name, std::path::Path::new(&p.path), &self.lint_engine) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
