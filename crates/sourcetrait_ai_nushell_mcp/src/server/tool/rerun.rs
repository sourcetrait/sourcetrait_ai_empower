use crate::*;

/// What: agent-facing parameters for `rerun()`. Carries the rerun_id
/// of a previously-cached closure plus fresh args for this
/// invocation.
///
/// Why: rerun lets an agent re-evaluate a known-good closure with
/// new args without re-sending the source -- saves tokens and pins
/// the body to the version that was originally tested. The
/// rerun_id is base62-validated before path-joining as defense in
/// depth against traversal.
///
/// Where: extracted via `mcp::Parameters<RerunParams>` in
/// `NuSh::rerun`; the rerun_id maps to a `closures/<rerun_id>.json`
/// cache file written by `write_closure_cache` during the original
/// run.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RerunParams {
    /// base62 rerun_id returned by a prior `run()` invocation. Names a
    /// `closures/<rerun_id>.json` cache file under
    /// `$XDG_CACHE_HOME/sourcetrait/nushell_mcp/`.
    pub rerun_id: String,
    /// Per-call args. The args_schema baked into the cached closure
    /// gates this at parse time inside the worker.
    pub args: mcp::JsonObject,
    /// Optional per-call timeout in milliseconds. Same semantics as
    /// `RunParams.timeout_ms`. Defaults to 120000 when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Success envelope for `rerun()`. The agent supplied the rerun_id
/// so we don't echo it back; just the result + a fresh nonce.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RerunEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

#[mcp::tool_router(router = rerun_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Re-evaluate a cached stateless closure body by rerun_id with new args.",
        output_schema = mcp::schema_for_type::<RerunEnvelope>()
    )]
    async fn rerun(
        &self,
        mcp::Parameters(p): mcp::Parameters<RerunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        if !lib_empower::is_base62(&p.rerun_id) {
            return Ok(error_to_call_result(
                Error::ClosureInvalidRerunId {
                    rerun_id: p.rerun_id.clone(),
                    reason: "rerun_id must be base62".to_string(),
                },
                None,
            ));
        }
        let path = closure_cache_file(&p.rerun_id);
        let cached_bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => return Ok(error_to_call_result(
                Error::ClosureCacheMissing {
                    rerun_id: p.rerun_id.clone(),
                },
                None,
            )),
        };
        let cached: ClosureCacheBody = match json::from_slice(&cached_bytes) {
            Ok(c) => c,
            Err(e) => return Ok(error_to_call_result(
                Error::ClosureCacheDecode {
                    rerun_id: p.rerun_id.clone(),
                    reason: e.to_string(),
                },
                None,
            )),
        };
        // Touch mtime for the LRU signal future pruning will use.
        // Idempotent overwrite -- content is deterministic.
        let _ = fs::write(&path, &cached_bytes);
        let source = build_run_source(
            &cached.args_type,
            &cached.result_type,
            &p.args,
            &cached.body,
        );
        let args_json = serde_json::Value::Object(p.args.clone());
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Runs,
            &cached_bytes,
            source,
            "rerun",
            args_json,
            InFlightKind::Rerun { rerun_id: p.rerun_id.clone() },
            p.timeout_ms,
        )
        .await
        {
            Ok(o) => o,
            Err(de) => return Ok(error_to_call_result(de.error, de.nonce)),
        };
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        envelope_to_structured(&RerunEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        })
    }
}
