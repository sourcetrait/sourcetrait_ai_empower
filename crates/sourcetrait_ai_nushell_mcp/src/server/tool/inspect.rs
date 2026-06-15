use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// Library to inspect.
    pub library: String,
    /// Slash-separated module path within the library; empty/omitted for
    /// the library root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// Function name. Omit to inspect a module (with `module_path`) or the
    /// library root (with neither).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Success result of `inspect()` -- the documentation for one node.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InspectEnvelope {
    pub library: String,
    pub module_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_schema: Option<mcp::JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_schema: Option<mcp::JsonObject>,
    /// The node's full detail documentation (empty when undocumented).
    pub details: String,
}

#[mcp::tool_router(router = inspect_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Detailed documentation of a specific callable library, module, function.",
        output_schema = mcp::schema_for_type::<InspectEnvelope>()
    )]
    async fn inspect(
        &self,
        mcp::Parameters(p): mcp::Parameters<InspectParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.lookup(&p.library).await {
            Some(l) => l,
            None => {
                return Ok(error_to_call_result(
                    Error::LibraryNotRegistered {
                        library: p.library.clone(),
                    },
                    None,
                ));
            }
        };
        let _guard = lock.read().await;
        let module_path = p.module_path.as_deref().unwrap_or("");
        let name = p.name.as_deref();
        match inspect_impl(&p.library, module_path, name) {
            Ok(r) => envelope_to_structured(&InspectEnvelope {
                library: r.library,
                module_path: r.module_path,
                name: r.name,
                summary: r.summary,
                args_schema: r.args_schema,
                result_schema: r.result_schema,
                details: r.details,
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
