use crate::*;

/// Parameters for `rerun()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RerunParams {
    /// The nonce a prior `run()` returned; names its cached body to re-evaluate.
    pub nonce: String,
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
    pub(crate) async fn rerun(
        &self,
        mcp::Parameters(p): mcp::Parameters<RerunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        if !lib_grammar::is_base62(&p.nonce) {
            return Ok(error_to_call_result(
                Error::RerunInvalidNonce {
                    nonce: p.nonce.clone(),
                    reason: "nonce must be base62".to_string(),
                },
                None,
            ));
        }
        let path = run_body_file(&p.nonce);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                return Ok(error_to_call_result(
                    Error::RerunBodyMissing {
                        nonce: p.nonce.clone(),
                    },
                    None,
                ));
            }
        };
        let cached = match CachedRunBody::from_nuon(&text) {
            Ok(c) => c,
            Err(reason) => {
                return Ok(error_to_call_result(
                    Error::RerunBodyDecode {
                        nonce: p.nonce.clone(),
                        reason,
                    },
                    None,
                ));
            }
        };
        let nonce = self.nonce_gen.next(&text);
        let source = build_run_source(
            &cached.args_type,
            &cached.result_type,
            &p.args,
            &cached.body,
            &nonce.to_string(),
        );
        let args_json = serde_json::Value::Object(p.args.clone());
        let cache_body = CachedRunBody {
            args_type: cached.args_type,
            result_type: cached.result_type,
            body: cached.body,
        };
        let outcome = match dispatch_pooled(
            &self.executor,
            &self.in_flight,
            &self.hung_watch,
            CacheKind::Runs,
            nonce,
            source,
            "rerun",
            args_json,
            InFlightKind::Rerun {
                source_nonce: p.nonce.clone(),
            },
            Some(cache_body),
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
