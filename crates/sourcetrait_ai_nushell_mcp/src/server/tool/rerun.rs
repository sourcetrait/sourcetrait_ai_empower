use crate::*;

/// Parameters for `rerun()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RerunParams {
    /// The id returned by a prior `run()`, naming the cached body to re-evaluate.
    pub rerun_id: String,
    /// JSON object of fresh argument values for this re-evaluation.
    pub args: mcp::JsonObject,
    /// Optional per-call timeout in milliseconds; defaults to 120000 (2 minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Success result of `rerun()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RerunEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

#[mcp::tool_router(router = rerun_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Re-evaluate a cached `run()` body with fresh args.",
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
            Err(_) => {
                return Ok(error_to_call_result(
                    Error::ClosureCacheMissing {
                        rerun_id: p.rerun_id.clone(),
                    },
                    None,
                ));
            }
        };
        let cached: ClosureCacheBody = match json::from_slice(&cached_bytes) {
            Ok(c) => c,
            Err(e) => {
                return Ok(error_to_call_result(
                    Error::ClosureCacheDecode {
                        rerun_id: p.rerun_id.clone(),
                        reason: e.to_string(),
                    },
                    None,
                ));
            }
        };
        // Touch mtime for the LRU signal future pruning will use.
        // Idempotent overwrite -- content is deterministic.
        let _ = fs::write(&path, &cached_bytes);
        let nonce = self.nonce_gen.next(&cached_bytes);
        let source = build_run_source(
            &cached.args_type,
            &cached.result_type,
            &p.args,
            &cached.body,
            &nonce.to_string(),
        );
        let args_json = serde_json::Value::Object(p.args.clone());
        let outcome = match dispatch_pooled(
            &self.runs_pool,
            &self.in_flight,
            CacheKind::Runs,
            nonce,
            source,
            "rerun",
            args_json,
            InFlightKind::Rerun {
                rerun_id: p.rerun_id.clone(),
            },
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
