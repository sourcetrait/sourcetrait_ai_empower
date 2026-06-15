use crate::*;

/// Agent-facing parameters for `inspect` - the doc lookup (leg 4).
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

/// Success envelope for `inspect`: a full single-node descriptor at any level
/// (library / module / function) - the coordinate (library, module_path,
/// optional name), the docs (summary + details, both empty when undocumented),
/// and, for a function, the call schemas (args_schema + result_schema).
/// `name` + the schemas are omitted for a module or the library root.
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
    /// Full doc body (after the summary's blank line). Last in the ordering -
    /// the longest, least-scannable field. Empty when undocumented.
    pub details: String,
}

#[mcp::tool_router(router = inspect_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Return the full doc (summary + details) for one node coordinate: a function (library + module_path + name), a module (library + module_path), or the library root (library only). Doc-only - no source, no schemas. info() carries the one-liner summary per node; inspect() is the on-demand full-doc lookup.",
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
