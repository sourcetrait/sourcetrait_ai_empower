use crate::*;

/// Parameters for `inspect()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InspectParams {
    /// A namepath at any arity, or a PATTERN ending in `/` or `:`, or `*` / `.`.
    pub namepath: String,
}

/// Success result of `inspect()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InspectEnvelope {
    /// An exact namepath's documentation, or a pattern's `signatures` block.
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
        let namepath = match NamepathStr::parse(&p.namepath) {
            Ok(NamepathStr::Pattern(pattern)) => {
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
                    GrammarMcpError::RigNotRegistered {
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
