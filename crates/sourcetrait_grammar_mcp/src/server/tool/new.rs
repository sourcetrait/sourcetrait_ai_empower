use crate::*;

/// Parameters for `new()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct NewParams {
    /// Namepaths to scaffold into existing libraries: `library:module/path`
    /// (a utility module) or `library:module/path:function` (a single-`main`
    /// function skeleton). Multiple may target multiple libraries.
    pub namepaths: Vec<String>,
}

/// Success result of `new()` -- every path created across the batch.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct NewEnvelope {
    pub created: Vec<String>,
}

#[mcp::tool_router(router = new_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        name = "new",
        description = "Scaffold modules / functions (by namepath) into existing libraries.",
        output_schema = mcp::schema_for_type::<NewEnvelope>()
    )]
    pub(crate) async fn scaffold(
        &self,
        mcp::Parameters(p): mcp::Parameters<NewParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let mut targets: Vec<(String, String, Option<String>)> = Vec::new();
        for np in &p.namepaths {
            match Namepath(np.clone()).validate() {
                Ok(NamepathRef::Module {
                    library,
                    module_path,
                }) => targets.push((library, module_path, None)),
                Ok(NamepathRef::Function {
                    library,
                    module_path,
                    name,
                }) => targets.push((library, module_path, Some(name))),
                Ok(NamepathRef::Library { .. }) => {
                    return Ok(error_to_call_result(
                        Error::NamepathInvalid {
                            namepath: np.clone(),
                            reason: "new() needs a module or function namepath; use library(new) to create a library"
                                .to_string(),
                        },
                        None,
                    ));
                }
                Err(e) => return Ok(error_to_call_result(e, None)),
            }
        }

        let mut libs: Vec<String> = targets.iter().map(|(l, _, _)| l.clone()).collect();
        libs.sort();
        libs.dedup();
        let mut locks = Vec::new();
        for lib in &libs {
            match self.library_locks.lookup(lib).await {
                Some(l) => locks.push(l),
                None => {
                    return Ok(error_to_call_result(
                        Error::LibraryNotRegistered {
                            library: lib.clone(),
                        },
                        None,
                    ));
                }
            }
        }
        let mut guards = Vec::new();
        for l in &locks {
            guards.push(l.write().await);
        }

        for (library, module_path, name) in &targets {
            match scaffold_leaf_exists(library, module_path, name.as_deref()) {
                Ok(true) => {
                    return Ok(error_to_call_result(
                        Error::LibraryInvalidName {
                            library: module_path.clone(),
                            reason: format!(
                                "`{}` already exists; edit it instead of scaffolding over it",
                                name.clone().unwrap_or_else(|| module_path.clone()),
                            ),
                        },
                        None,
                    ));
                }
                Ok(false) => {}
                Err(e) => return Ok(error_to_call_result(e, None)),
            }
        }

        let mut created = Vec::new();
        for (library, module_path, name) in &targets {
            match scaffold_leaf(library, module_path, name.as_deref()) {
                Ok(mut c) => created.append(&mut c),
                Err(e) => return Ok(error_to_call_result(e, None)),
            }
        }
        envelope_to_structured(&NewEnvelope { created })
    }
}
