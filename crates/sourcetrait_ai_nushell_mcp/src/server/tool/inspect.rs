use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// The coordinate's namepath: `library`, `library:module/path`, or
    /// `library:module/path:function`.
    pub namepath: String,
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
        // namepath -> structured coordinate. inspect accepts any arity:
        // library / module / function.
        let (library, module_path, name) = match Namepath(p.namepath.clone()).validate() {
            Ok(NamepathRef::Library { library }) => (library, String::new(), None),
            Ok(NamepathRef::Module {
                library,
                module_path,
            }) => (library, module_path, None),
            Ok(NamepathRef::Function {
                library,
                module_path,
                name,
            }) => (library, module_path, Some(name)),
            Err(e) => return Ok(error_to_call_result(e, None)),
        };
        let lock = match self.library_locks.lookup(&library).await {
            Some(l) => l,
            None => {
                return Ok(error_to_call_result(
                    Error::LibraryNotRegistered {
                        library: library.clone(),
                    },
                    None,
                ));
            }
        };
        let _guard = lock.read().await;
        match inspect_impl(&library, &module_path, name.as_deref()) {
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
