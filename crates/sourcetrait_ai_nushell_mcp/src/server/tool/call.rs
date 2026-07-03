use crate::*;

/// Parameters for `call()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CallParams {
    /// The function's namepath: `library:module/path:function`.
    pub namepath: String,
    /// JSON object of argument values passed to the function as `$args`.
    pub args: mcp::JsonObject,
    /// Optional per-call timeout in milliseconds; defaults to 120000 (2 minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Success result of `call()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CallEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

#[mcp::tool_router(router = call_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Invoke a committed library function with typed args.",
        output_schema = mcp::schema_for_type::<CallEnvelope>()
    )]
    pub(crate) async fn call(
        &self,
        mcp::Parameters(p): mcp::Parameters<CallParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // call requires a FUNCTION namepath (library:module/path:function).
        let (library, module_path, name) = match Namepath(p.namepath.clone()).validate() {
            Ok(NamepathRef::Function {
                library,
                module_path,
                name,
            }) => (library, module_path, name),
            Ok(_) => {
                return Ok(error_to_call_result(
                    Error::NamepathInvalid {
                        namepath: p.namepath.clone(),
                        reason: "call requires a function namepath: library:module/path:function"
                            .to_string(),
                    },
                    None,
                ));
            }
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
        // big meta: the index is the callability authority - the coordinate
        // must name a registered call-target. A helper file present on disk
        // but absent from the index is correctly NOT callable.
        let index = match load_index(&library) {
            Ok(i) => i,
            Err(e) => {
                return Ok(error_to_call_result(
                    Error::Internal {
                        phase: "call::load_index".to_string(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        let is_call_target = index_node(&index, &module_path)
            .map(|(fns, _)| fns.iter().any(|f| f.name == name))
            .unwrap_or(false);
        if !is_call_target {
            return Ok(error_to_call_result(
                Error::FunctionNotDefined {
                    library: library.clone(),
                    module_path: module_path.clone(),
                    name: name.clone(),
                },
                None,
            ));
        }
        let payload_bytes = match json::to_vec(&p) {
            Ok(b) => b,
            Err(e) => {
                return Ok(error_to_call_result(
                    Error::Internal {
                        phase: "call::serialize_payload".to_string(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        // Mint the nonce BEFORE source synthesis so it can be embedded as
        // $env.NONCE in the call template.
        let nonce = self.nonce_gen.next(&payload_bytes);
        let source = build_call_source(
            &library,
            &module_path,
            &name,
            &p.args,
            &nonce.to_string(),
        );
        // The in-flight path IS the namepath (a function namepath is always
        // library:module/path:name; there are no root functions).
        let path_str = p.namepath.clone();
        let args_json = serde_json::Value::Object(p.args.clone());
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.in_flight,
            CacheKind::Calls,
            nonce,
            source,
            "call",
            args_json,
            InFlightKind::Call { path: path_str },
            p.timeout_ms,
        )
        .await
        {
            Ok(o) => o,
            Err(de) => return Ok(error_to_call_result(de.error, de.nonce)),
        };
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        envelope_to_structured(&CallEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        })
    }
}
