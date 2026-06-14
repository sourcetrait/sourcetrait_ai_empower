use crate::*;

/// What: agent-facing success envelope for `run()`. Carries the
/// closure's typed return value (per `result_schema`), the per-call
/// nonce, and the content-derived `rerun_id` when caching succeeded.
///
/// Why: a typed struct (rather than ad-hoc `serde_json::json!`)
/// gives schemars an outputSchema to publish on `run`'s tool
/// descriptor, lets rmcp emit the value via `structured_content`,
/// and lets future-me reason about the envelope by name rather than
/// by JSON key lookup.
///
/// Where: returned from `NuSh::run` wrapped in a `CallToolResult`
/// whose `structured_content` field carries the serialized
/// envelope. The matching `outputSchema` is declared on the
/// `#[mcp::tool]` attribute via `schema_for_type::<RunEnvelope>()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RunEnvelope {
    /// Worker-evaluated return value of the body. Always a JSON object
    /// because the worker template runs `__resolve [result: record<...>]
    /// $result` which forces a record-shaped return. `mcp::JsonObject`
    /// (not `serde_json::Value`) keeps the schemars rendering as
    /// `{"type": "object"}` -- the `true` rendering Value would produce
    /// is rejected by Claude Code's MCP client schema validator (same
    /// gotcha as `RunParams.args`).
    pub result: mcp::JsonObject,
    pub nonce: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_id: Option<String>,
}

#[mcp::tool_router(router = run_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Evaluate a typed nushell closure body on a stateless worker.",
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
        let violations = lint_run_params(&self.lint_engine, &args_type, &p.body);
        if !violations.is_empty() {
            return Ok(error_to_call_result(
                Error::LintViolations { violations },
                None,
            ));
        }
        let source = build_run_source(&args_type, &result_type, &p.args, &p.body);
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
        let args_json = serde_json::Value::Object(p.args.clone());
        let timeout_ms = p.timeout_ms;
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Runs,
            &payload_bytes,
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
        let computed_rerun_id = lib_empower::RerunHash::of(&(
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
