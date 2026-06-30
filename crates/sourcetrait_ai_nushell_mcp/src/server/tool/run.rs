use crate::*;

/// Success result of `run()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RunEnvelope {
    /// The source-code body's return value, as a JSON object matching `result_schema`.
    // `mcp::JsonObject` (not `serde_json::Value`): same JsonSchema-rendering gotcha as RunParams.args.
    pub result: mcp::JsonObject,
    pub nonce: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_id: Option<String>,
}

#[mcp::tool_router(router = run_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Evaluate a typed nushell source-code body on a stateless worker.",
        output_schema = mcp::schema_for_type::<RunEnvelope>()
    )]
    async fn run(
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
        // Mint the nonce BEFORE source synthesis so it can be embedded as
        // $env.NONCE in the run template.
        let nonce = self.nonce_gen.next(&payload_bytes);
        let source =
            build_run_source(&args_type, &result_type, &p.args, &p.body, &nonce.to_string());
        let args_json = serde_json::Value::Object(p.args.clone());
        let timeout_ms = p.timeout_ms;
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.in_flight,
            CacheKind::Runs,
            nonce,
            source,
            "run",
            args_json,
            InFlightKind::Run,
            timeout_ms,
        )
        .await
        {
            Ok(o) => o,
            Err(de) => return Ok(error_to_call_result(de.error, de.nonce)),
        };
        let computed_rerun_id = RerunHash::of(&(
            args_type.as_str(),
            result_type.as_str(),
            p.body.as_str(),
        ))
        .to_string();
        let rerun_id_opt =
            match write_closure_cache(&computed_rerun_id, &args_type, &result_type, &p.body) {
                Ok(()) => Some(computed_rerun_id),
                Err(e) => {
                    eprintln!(
                        "nushell_mcp: write_closure_cache failed for {computed_rerun_id}: {e}",
                    );
                    None
                }
            };
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        let envelope = RunEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
            rerun_id: rerun_id_opt,
        };
        envelope_to_structured(&envelope)
    }
}

/// What: writes the closure cache file at `closures/<rerun_id>.json`
/// after a successful `run()`. Creates the parent dir if needed,
/// serializes the closure metadata (`args_type`, `result_type`,
/// `body`) into `ClosureCacheBody`, and writes the JSON bytes.
/// Idempotent: the same rerun_id always produces the same bytes.
///
/// Why: rerun() needs a deterministic place to look up the cached
/// closure by its content-derived id. The unconditional overwrite
/// touches mtime even on identical content, which sets up future
/// LRU-style pruning. Returns `io::Result<()>` (slice 6.0): the caller
/// treats failures as non-fatal -- the agent still gets the successful
/// eval result, with the `rerun_id` field omitted from the envelope so
/// the agent knows replay is unavailable for this call.
///
/// Where: called by `NuSh::run` after `dispatch_pooled` succeeds
/// and `RerunHash::of` produces the rerun_id. The matching read
/// happens in `NuSh::rerun` via `fs::read` + `json::from_slice`.
fn write_closure_cache(
    rerun_id: &str,
    args_type: &str,
    result_type: &str,
    body: &str,
) -> io::Result<()> {
    let path = closure_cache_file(rerun_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = ClosureCacheBody {
        args_type: args_type.to_string(),
        result_type: result_type.to_string(),
        body: body.to_string(),
    };
    let bytes = json::to_vec(&cache)
        .map_err(|e| io::Error::other(format!("serialize ClosureCacheBody: {e}")))?;
    fs::write(&path, &bytes)?;
    Ok(())
}
