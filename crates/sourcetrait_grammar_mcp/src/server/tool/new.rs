use crate::*;

/// Parameters for `new()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct NewParams {
    /// Namepaths to scaffold into existing rigs: a module or a function.
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
        description = "Scaffold modules / functions (by namepath) into existing rigs.",
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
                    rig,
                    module_path,
                }) => targets.push((rig, module_path, None)),
                Ok(NamepathRef::Function {
                    rig,
                    module_path,
                    name,
                }) => targets.push((rig, module_path, Some(name))),
                Ok(NamepathRef::Rig { .. }) => {
                    return Ok(error_to_call_result(
                        Error::NamepathInvalid {
                            namepath: np.clone(),
                            reason: "new() needs a module or function namepath; use rig(new) to create a rig"
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
            match self.rig_locks.lookup(lib).await {
                Some(l) => locks.push(l),
                None => {
                    return Ok(error_to_call_result(
                        Error::RigNotRegistered {
                            rig: lib.clone(),
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

        for (rig, module_path, name) in &targets {
            match scaffold_leaf_exists(rig, module_path, name.as_deref()) {
                Ok(true) => {
                    return Ok(error_to_call_result(
                        Error::RigInvalidName {
                            rig: module_path.clone(),
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
        for (rig, module_path, name) in &targets {
            match scaffold_leaf(rig, module_path, name.as_deref()) {
                Ok(mut c) => created.append(&mut c),
                Err(e) => return Ok(error_to_call_result(e, None)),
            }
        }
        envelope_to_structured(&NewEnvelope { created })
    }
}
