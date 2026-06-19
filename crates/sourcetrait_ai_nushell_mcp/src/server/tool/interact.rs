use crate::*;

/// Success result of `interact()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InteractEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

#[mcp::tool_router(router = interact_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Evaluate a typed nushell source-code body on a persistent stateful worker.",
        output_schema = mcp::schema_for_type::<InteractEnvelope>()
    )]
    async fn interact(
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
                        phase: "interact::serialize_payload".to_string(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        // Mint the nonce BEFORE source synthesis so it can be embedded as
        // $env.NONCE in the interact template.
        let nonce = self.nonce_gen.next(&payload_bytes);
        let source =
            build_interact_source(&args_type, &result_type, &p.args, &p.body, &nonce.to_string());
        let args_json = serde_json::Value::Object(p.args.clone());
        let timeout_ms = p.timeout_ms;
        let outcome = match dispatch_interact(
            &self.interact_worker,
            &self.in_flight,
            nonce,
            source,
            args_json,
            timeout_ms,
        )
        .await
        {
            Ok(o) => o,
            Err(de) => return Ok(error_to_call_result(de.error, de.nonce)),
        };
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        let envelope = InteractEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        };
        envelope_to_structured(&envelope)
    }
}
