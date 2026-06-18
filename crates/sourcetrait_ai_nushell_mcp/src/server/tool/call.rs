use crate::*;

/// Parameters for `call()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CallParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
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
    async fn call(
        &self,
        mcp::Parameters(p): mcp::Parameters<CallParams>,
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
        let file_path = match call_file_path(&p.library, &p.module_path, &p.name) {
            Some(p) => p,
            None => {
                return Ok(error_to_call_result(
                    Error::LibraryInvalidModulePath {
                        module_path: p.module_path.clone(),
                        reason: "library / module_path / name must satisfy identifier rules"
                            .to_string(),
                    },
                    None,
                ));
            }
        };
        // big meta: the index is the callability authority - the coordinate
        // must name a registered call-target. A helper file present on disk
        // but absent from the index is correctly NOT callable.
        let index = match load_index(&p.library) {
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
        let is_call_target = index_node(&index, &p.module_path)
            .map(|(fns, _)| fns.iter().any(|f| f.name == p.name))
            .unwrap_or(false);
        if !is_call_target {
            return Ok(error_to_call_result(
                Error::FunctionNotDefined {
                    library: p.library.clone(),
                    module_path: p.module_path.clone(),
                    name: p.name.clone(),
                },
                None,
            ));
        }
        let source = build_call_source(&file_path.display().to_string(), &p.name, &p.args);
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
        let path_str = if p.module_path.is_empty() {
            format!("{}::{}", p.library, p.name)
        } else {
            format!("{}:{}:{}", p.library, p.module_path, p.name)
        };
        let args_json = serde_json::Value::Object(p.args.clone());
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Calls,
            &payload_bytes,
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
