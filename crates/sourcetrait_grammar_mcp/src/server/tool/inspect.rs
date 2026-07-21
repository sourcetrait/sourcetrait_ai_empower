use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// The coordinate's namepath: `library`, `library:module/path`, or
    /// `library:module/path:function`.
    pub namepath: String,
}

/// Success result of `inspect()` -- the documentation for one node.
///
/// EXACTLY one field. The namepath is not reprinted, because the caller supplied
/// it, and the per-kind shapes carry nothing in common worth hoisting.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InspectEnvelope {
    /// The node's documentation: a library (`srcdir`), a module (`src`), or a
    /// call (`src` + its full `signature`). Each also carries `details`, and a
    /// library and module their `summary`; a call's summary rides its signature.
    pub doc: InspectDoc,
}

#[mcp::tool_router(router = inspect_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Detailed documentation of a specific callable library, module, function.",
        output_schema = mcp::schema_for_type::<InspectEnvelope>()
    )]
    pub(crate) async fn inspect(
        &self,
        mcp::Parameters(p): mcp::Parameters<InspectParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
            Ok(doc) => envelope_to_structured(&InspectEnvelope { doc }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
