use crate::*;

/// Agent-facing parameters for `new` - the scaffold tool (leg 3).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct NewParams {
    /// Library name. The FIRST new() for a name establishes the library.
    pub library: String,
    /// Absolute path to the agent's source tree. REQUIRED on the
    /// establishing (first) call for a library name; omit it thereafter
    /// (it is fixed in the library meta at establishment).
    pub source_path: Option<String>,
    /// Slash-separated module path within the library; empty/omitted for
    /// the library root.
    pub module_path: Option<String>,
    /// Function name to scaffold (the call/resolve/main skeleton). Omit to
    /// scaffold only the module level.
    pub name: Option<String>,
}

/// Success envelope for `new`: the source tree it scaffolded into + the
/// paths it created (the agent now edits these, then commit()s).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct NewEnvelope {
    pub source_path: String,
    pub created: Vec<String>,
}

#[mcp::tool_router(router = new_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        name = "new",
        description = "Scaffold a library / module / function into the agent's source tree (the call/resolve/main skeleton with record<> placeholders). The FIRST call for a library name establishes it + records source_path (required then, immutable after). Purely additive - refuses to scaffold over an existing leaf; edit the files, then commit().",
        output_schema = mcp::schema_for_type::<NewEnvelope>()
    )]
    async fn scaffold(
        &self,
        mcp::Parameters(p): mcp::Parameters<NewParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Get-or-create the per-library write lock: register on the first
        // (establishing) call, look up the existing lock thereafter.
        let lock = match self.library_locks.register(&p.library).await {
            Ok(l) => l,
            Err(_) => match self.library_locks.lookup(&p.library).await {
                Some(l) => l,
                None => return Ok(error_to_call_result(
                    Error::Internal {
                        phase: "new::lock".to_string(),
                        reason: format!("could not acquire the write lock for `{}`", p.library),
                    },
                    None,
                )),
            },
        };
        let _guard = lock.write().await;
        let source_path = p.source_path.as_deref().map(std::path::Path::new);
        let module_path = p.module_path.as_deref().unwrap_or("");
        let name = p.name.as_deref();
        match new_impl(&p.library, source_path, module_path, name) {
            Ok(result) => envelope_to_structured(&NewEnvelope {
                source_path: result.source_path,
                created: result.created,
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
