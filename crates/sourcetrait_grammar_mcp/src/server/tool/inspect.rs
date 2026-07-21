use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// A coordinate's namepath - `library`, `library:module/path`, or
    /// `library:module/path:function` - or a PATTERN, which ends in a trailing
    /// `/` or `:` (`author/`, `library:`, `library:module/`, `library:module:`)
    /// or is the bare `*`.
    pub namepath: String,
}

/// Success result of `inspect()` -- the documentation for one node, or the
/// signature block for a pattern.
///
/// EXACTLY one field. The namepath is not reprinted, because the caller supplied
/// it, and the per-kind shapes carry nothing in common worth hoisting.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InspectEnvelope {
    /// An exact coordinate's documentation - a library (`srcdir`), a module
    /// (`src`), or a call (`src` + its full `signature`), each also carrying
    /// `details`, with `summary` on the library and module while a call's rides
    /// its signature - or, for a PATTERN, the `signatures` block rooted there.
    pub doc: InspectDoc,
}

#[mcp::tool_router(router = inspect_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Detailed documentation for libraries, modules, and calls.",
        output_schema = mcp::schema_for_type::<InspectEnvelope>()
    )]
    pub(crate) async fn inspect(
        &self,
        mcp::Parameters(p): mcp::Parameters<InspectParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Classification is by SHAPE and happens first: a trailing hierarchy
        // character - or a bare `*` / `.` - makes this a PATTERN, and anything
        // else validates as the exact coordinate it always did.
        let namepath = match NamepathStr::parse(&p.namepath) {
            Ok(NamepathStr::Pattern(pattern)) => {
                // A pattern spans libraries, so the per-library READ locks are
                // taken inside the renderer rather than around one lookup here.
                let signatures =
                    render_signatures_matching(&self.library_locks, &pattern).await;
                return envelope_to_structured(&InspectEnvelope {
                    doc: InspectDoc::Signatures(SignaturesDoc { signatures }),
                });
            }
            Ok(NamepathStr::Namepath(n)) => n,
            Err(e) => return Ok(error_to_call_result(e, None)),
        };
        let (library, module_path, name) = match namepath.validate() {
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
