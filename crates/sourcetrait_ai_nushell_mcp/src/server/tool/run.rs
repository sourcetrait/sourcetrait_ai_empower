use crate::*;

/// Success result of `run()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RunEnvelope {
    /// The source-code body's return value, as a JSON object matching `result_schema`.
    pub result: mcp::JsonObject,
    /// This eval's id, and its re-evaluation handle: `rerun(nonce, args)` replays
    /// this body with fresh args. Also names the per-call log dir + cached body.
    pub nonce: String,
}

#[mcp::tool_router(router = run_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Evaluate a typed nushell source-code body on a stateless worker.",
        output_schema = mcp::schema_for_type::<RunEnvelope>()
    )]
    pub(crate) async fn run(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let (args_type, result_type) = match convert_schemas(&p.args_schema, &p.result_schema) {
            Ok(t) => t,
            Err(reason) => return Ok(error_to_call_result(Error::SchemaInvalid { reason }, None)),
        };
        let diagnostics = lint_run_params(&self.lint_engine, &args_type, &p.body);
        if !diagnostics.is_empty() {
            return Ok(error_to_call_result(
                Error::LintViolations { diagnostics },
                None,
            ));
        }
        let payload_bytes = match json::to_vec(&p) {
            Ok(b) => b,
            Err(e) => {
                return Ok(error_to_call_result(
                    Error::Internal {
                        phase: "run::serialize_payload".to_string(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        let nonce = self.nonce_gen.next(&payload_bytes);
        let source =
            build_run_source(&args_type, &result_type, &p.args, &p.body, &nonce.to_string());
        let args_json = serde_json::Value::Object(p.args.clone());
        let cache_body = CachedRunBody {
            args_type,
            result_type,
            body: p.body,
        };
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.in_flight,
            CacheKind::Runs,
            nonce,
            source,
            "run",
            args_json,
            InFlightKind::Run,
            Some(cache_body),
            p.timeout_ms,
        )
        .await
        {
            Ok(o) => o,
            Err(de) => return Ok(error_to_call_result(de.error, de.nonce)),
        };
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        let envelope = RunEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        };
        envelope_to_structured(&envelope)
    }
}
