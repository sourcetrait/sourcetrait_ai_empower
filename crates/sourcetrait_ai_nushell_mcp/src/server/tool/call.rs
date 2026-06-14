use crate::*;

/// What: agent-facing parameters for `call`. Carries the
/// (library, module_path, name) coordinate of the function to invoke
/// plus the args object to pass as `$args`.
///
/// Why: call routes through the stateless worker with a synthesized
/// template `use <abs path>; <name> resolve (<name> call ARGS_JSON)`
/// so the function's result_schema typecheck runs on every invocation,
/// against the RAW `call` (never `main`) for boundary enforcement.
/// HEAD-only -- no version pinning per the slice 3 lock.
///
/// Where: extracted in `NuSh::call`; the coordinate is path-validated
/// via `call_file_path`, the args become a record literal in the
/// synthesized source.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CallParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
    /// JSON object passed as `$args` to the function. Schema match is
    /// enforced by the function's `main` signature at parse time inside
    /// the worker (typed positional binding on a literal record).
    pub args: mcp::JsonObject,
    /// Optional per-call timeout in milliseconds. Same semantics as
    /// `RunParams.timeout_ms`. Defaults to 120000 when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Success envelope for `call()`. Carries the registered library
/// function's typed return value plus the per-call nonce. No
/// `rerun_id` (library calls are themselves the replayable unit;
/// recipe is `call(library, module_path, name, args)`).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CallEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

#[mcp::tool_router(router = call_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Invoke a registered library function on a stateless worker. Builds `use <abs path to function file>.nu; <name> resolve (<name> call $args)` - targeting the raw `call` so a mis-authored `main` cannot leak an unvalidated value, with the function's `resolve` typecheck run on the boundary. HEAD-only -- no version pinning.",
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
        if !file_path.exists() {
            return Ok(error_to_call_result(
                Error::FunctionNotDefined {
                    library: p.library.clone(),
                    module_path: p.module_path.clone(),
                    name: p.name.clone(),
                },
                None,
            ));
        }
        let args_json_str = if p.args.is_empty() {
            "null".to_string()
        } else {
            json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string())
        };
        let source = format!(
            "use {}\n{} resolve ({} call {})\n",
            file_path.display(),
            p.name,
            p.name,
            args_json_str,
        );
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
