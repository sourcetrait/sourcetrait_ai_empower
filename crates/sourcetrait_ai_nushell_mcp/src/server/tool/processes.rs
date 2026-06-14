use crate::*;

/// What: agent-facing parameters for `processes`. Empty -- the tool
/// takes no input. Returns a snapshot of every in-flight call on the
/// host.
///
/// Why: an empty params struct (`{}`) is the schemars-friendly shape
/// rmcp expects for a no-arg tool; not having any params keeps the
/// tool surface explicit.
///
/// Where: extracted in `NuSh::processes`; the body just snapshots the
/// in-flight map and serializes per-tool entry shapes.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ProcessesParams {}

/// Per-tool-call entry returned in the `processes()` snapshot.
/// `args` is always object-shaped (matches the agent's submission
/// shape for run/interact/call/rerun). `rerun_id` populated only
/// for rerun calls; `path` only for call calls.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ProcessEntry {
    pub nonce: String,
    pub tool: String,
    pub started_at: u64,
    pub args: mcp::JsonObject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Success envelope for `processes()`. The snapshot field carries
/// zero or more `ProcessEntry` records, one per in-flight tool
/// call.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ProcessesEnvelope {
    pub processes: Vec<ProcessEntry>,
}

#[mcp::tool_router(router = processes_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Snapshot every in-flight tool call on the host. Returns an array of entries with the shape {nonce, tool, started_at, args, ...tool-specific}: for `run`/`interact` no extras; for `rerun` includes `rerun_id`; for `call` includes a flat `path` string `library:module/path:name` (with `library::name` when module_path is empty). Pair with `kill(nonce)` to cancel a specific call.",
        output_schema = mcp::schema_for_type::<ProcessesEnvelope>()
    )]
    async fn processes(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ProcessesParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let map = self.in_flight.lock().await;
        let entries: Vec<ProcessEntry> = map
            .iter()
            .map(|(nonce_str, entry)| {
                let args_obj = entry.args.as_object().cloned().unwrap_or_default();
                let (rerun_id, path) = match &entry.kind {
                    InFlightKind::Run | InFlightKind::Interact => (None, None),
                    InFlightKind::Rerun { rerun_id } => (Some(rerun_id.clone()), None),
                    InFlightKind::Call { path } => (None, Some(path.clone())),
                };
                ProcessEntry {
                    nonce: nonce_str.clone(),
                    tool: entry.tool.to_string(),
                    started_at: entry.started_at,
                    args: args_obj,
                    rerun_id,
                    path,
                }
            })
            .collect();
        drop(map);
        envelope_to_structured(&ProcessesEnvelope { processes: entries })
    }
}
