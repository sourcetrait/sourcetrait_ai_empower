use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// A namepath - `rig`, `rig:module/path`, or
    /// `rig:module/path:function` - or a PATTERN, which ends in a trailing
    /// `/` or `:` (`author/`, `rig:`, `rig:module/`, `rig:module:`)
    /// or is the bare `*` (everything) or `.` (the current purview).
    pub namepath: String,
}

/// Success result of `inspect()` -- the documentation for one namepath, or the
/// signature block for a pattern.
///
/// EXACTLY one field. The namepath is not reprinted, because the caller supplied
/// it, and the per-kind shapes carry nothing in common worth hoisting.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InspectEnvelope {
    /// An exact namepath's documentation - a rig (`srcdir`), a module
    /// (`src`), or a call (`src` + its full `signature`), each also carrying
    /// `details`, with `summary` on the rig and module while a call's rides
    /// its signature - or, for a PATTERN, the `signatures` block rooted there.
    pub doc: InspectDoc,
}

#[mcp::tool_router(router = inspect_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Detailed documentation for rigs, modules, and calls.",
        output_schema = mcp::schema_for_type::<InspectEnvelope>()
    )]
    pub(crate) async fn inspect(
        &self,
        mcp::Parameters(p): mcp::Parameters<InspectParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Classification is by SHAPE and happens first: a trailing hierarchy
        // character - or a bare `*` / `.` - makes this a PATTERN, and anything
        // else validates as the exact namepath it always did.
        let namepath = match NamepathStr::parse(&p.namepath) {
            Ok(NamepathStr::Pattern(pattern)) => {
                // `.` is the CURRENT PURVIEW, and purview is the only thing that
                // knows what it names: the parser leaves it unresolved and an
                // unresolved `.` matches nothing, so resolving it HERE is what
                // makes `inspect(".")` the current view instead of an empty
                // block.
                let patterns = match pattern {
                    NamepathPattern::Current => {
                        let rows = match load_purviews() {
                            Ok(rows) => rows,
                            Err(error) => return Ok(error_to_call_result(error, None)),
                        };
                        parse_patterns(&expand_values(
                            &resolve_patterns(&self.current_purview.ids(), rows.as_ref()),
                            rows.as_ref(),
                        ))
                    }
                    other => vec![NamepathStr::Pattern(other)],
                };
                // A pattern spans rigs, so the per-rig READ locks are
                // taken inside the renderer rather than around one lookup here.
                let signatures =
                    render_signatures_within(&self.rig_locks, &patterns).await;
                return envelope_to_structured(&InspectEnvelope {
                    doc: InspectDoc::Signatures(SignaturesDoc { signatures }),
                });
            }
            Ok(NamepathStr::Namepath(n)) => n,
            Err(e) => return Ok(error_to_call_result(e, None)),
        };
        let (rig, module_path, name) = match namepath.validate() {
            Ok(NamepathRef::Rig { rig }) => (rig, String::new(), None),
            Ok(NamepathRef::Module {
                rig,
                module_path,
            }) => (rig, module_path, None),
            Ok(NamepathRef::Function {
                rig,
                module_path,
                name,
            }) => (rig, module_path, Some(name)),
            Err(e) => return Ok(error_to_call_result(e, None)),
        };
        let lock = match self.rig_locks.lookup(&rig).await {
            Some(l) => l,
            None => {
                return Ok(error_to_call_result(
                    Error::RigNotRegistered {
                        rig: rig.clone(),
                    },
                    None,
                ));
            }
        };
        let _guard = lock.read().await;
        match inspect_impl(&rig, &module_path, name.as_deref()) {
            Ok(doc) => envelope_to_structured(&InspectEnvelope { doc }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
