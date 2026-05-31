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
    pub closure: String,
}

pub struct NuSh {
    runs_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    interact_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    nonce_gen: Arc<lib_empower::NonceGen>,
    #[allow(dead_code)]
    tool_router: mcp::ToolRouter<NuSh>,
}

#[mcp::tool_router]
impl NuSh {
    pub(crate) fn new(
        runs_worker: WorkerHandle,
        interact_worker: WorkerHandle,
        nonce_gen: Arc<lib_empower::NonceGen>,
    ) -> Self {
        Self {
            runs_worker: Arc::new(tk::AsyncMutex::new(runs_worker)),
            interact_worker: Arc::new(tk::AsyncMutex::new(interact_worker)),
            nonce_gen,
            tool_router: Self::tool_router(),
        }
    }

    #[mcp::tool(
        description = "Evaluate a typed nushell closure on a stateless worker."
    )]
    async fn run(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<String, mcp::ErrorData> {
        let source = build_run_source(&p);
        dispatch(&self.runs_worker, &self.nonce_gen, CacheKind::Runs, &p, source).await
    }

    #[mcp::tool(
        description = "Evaluate a typed administrative nushell closure on a persistent stateful worker."
    )]
    async fn interact(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<String, mcp::ErrorData> {
        let source = build_interact_source(&p);
        dispatch(&self.interact_worker, &self.nonce_gen, CacheKind::Interacts, &p, source).await
    }
}

/// Shared dispatch path for `run()` and `interact()`. Handles nonce
/// derivation, log dir creation, worker round-trip, and result envelope
/// construction. The only per-tool variation lives at the call site:
/// which worker handle, which CacheKind, and which template builder
/// produced the source.
async fn dispatch(
    worker: &Arc<tk::AsyncMutex<WorkerHandle>>,
    nonce_gen: &lib_empower::NonceGen,
    kind: CacheKind,
    p: &RunParams,
    source: String,
) -> Result<String, mcp::ErrorData> {
    let payload_bytes = json::to_vec(p).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("serialize RunParams for nonce: {e}"),
            None,
        )
    })?;
    let nonce = nonce_gen.next(&payload_bytes);
    let log_dir = cache_dir(kind, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("create_dir_all {}: {e}", log_dir.display()),
            None,
        )
    })?;
    let mut worker_guard = worker.lock().await;
    let response = worker_guard
        .send_request(log_dir, source)
        .await
        .map_err(|e| mcp::ErrorData::internal_error(e.to_string(), None))?;
    drop(worker_guard);
    if response.ok {
        let value: json::Value = msgpack::from_slice(&response.value)
            .unwrap_or(json::Value::Null);
        let envelope = json::json!({
            "result": value,
            "nonce": nonce.to_string(),
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
        )
        .with_title("nushell");
        info.instructions = Some(
            "Evaluation artifacts are cached at \
             $XDG_CACHE_HOME/sourcetrait/nu_sh_mcp/{runs,interacts}/<nonce>/{stdout,stderr}."
                .to_string(),
        );
        info
    }
}
