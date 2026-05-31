use crate::*;

#[derive(Debug, Clone, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct Function {
    pub name: String,
    pub args_schema: String,
    pub result_schema: String,
    pub body: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RunParams {
    pub args_schema: String,
    pub result_schema: String,
    /// JSON object that becomes the nushell `$args` record literal at the
    /// __exec call site. Schemars represents `serde_json::Value` as the
    /// JSON Schema 2020-12 `true` keyword (match-anything), which Claude
    /// Code's MCP client rejects with "Invalid input" -- so we narrow the
    /// type to a Map (rmcp's `JsonObject` alias) which schemars renders
    /// as `{"type": "object"}`. Semantically correct anyway: args MUST be
    /// an object because it has to deserialize into a nushell record.
    pub args: mcp::JsonObject,
    #[serde(default)]
    pub functions: Vec<Function>,
    pub closure_body: String,
}

pub struct NuSh {
    worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    #[allow(dead_code)]
    tool_router: mcp::ToolRouter<NuSh>,
}

#[mcp::tool_router]
impl NuSh {
    pub(crate) fn new(worker: WorkerHandle) -> Self {
        Self {
            worker: Arc::new(tk::AsyncMutex::new(worker)),
            tool_router: Self::tool_router(),
        }
    }

    #[mcp::tool(
        description = "Evaluate a typed nushell closure on a worker. \
                       Builds a do-scoped __exec/__resolve template from \
                       args_schema, result_schema, args, optional helper \
                       functions, and the closure body. Returns the typed \
                       result plus a rerun_id."
    )]
    async fn run(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<String, mcp::ErrorData> {
        let source = build_run_source(&p);
        let mut worker = self.worker.lock().await;
        let response = worker.send_request(source).await.map_err(|e| {
            mcp::ErrorData::internal_error(e.to_string(), None)
        })?;
        drop(worker);
        if response.ok {
            let value: json::Value = msgpack::from_slice(&response.value)
                .unwrap_or(json::Value::Null);
            let envelope = json::json!({
                "result": value,
                "rerun_id": "0",
            });
            json::to_string_json(&envelope).map_err(|e| {
                mcp::ErrorData::internal_error(
                    format!("envelope serialize: {e}"),
                    None,
                )
            })
        } else {
            Err(mcp::ErrorData::internal_error(
                response.error.unwrap_or_else(|| {
                    "worker returned ok=false with no error".to_string()
                }),
                None,
            ))
        }
    }
}

#[mcp::tool_handler]
impl mcp::ServerHandler for NuSh {
    fn get_info(&self) -> mcp::ServerInfo {
        // Per [[rmcp-implementation-from-build-env-gotcha]]: must construct
        // Implementation via env! in THIS crate's source so the macros expand
        // against our CARGO_PKG_*, not rmcp's.
        //
        // ServerCapabilities::default() does NOT include `tools: Some(...)` --
        // empirically confirmed 2026-05-31 when Claude Code's MCP client
        // skipped tools/list after restart because the initialize response
        // advertised no tool capability. Explicit `.enable_tools()` is
        // required for the client to discover our tool surface.
        let mut info = mcp::ServerInfo::default();
        info.capabilities = mcp::ServerCapabilities::builder()
            .enable_tools()
            .build();
        info.server_info = mcp::Implementation::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        );
        info
    }
}
