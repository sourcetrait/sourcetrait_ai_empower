use crate::*;

/// What: agent-facing parameters for `kill`. The `nonce` is the value
/// returned in a prior tool envelope (the per-call id rendered as a
/// base62 string).
///
/// Why: cancellation needs to address one specific in-flight call;
/// `nonce` is the existing per-call id we already give the agent in
/// every envelope, so no new identifier is needed. Agent uses
/// `processes()` to discover live nonces, matches against their own
/// send-set via `args`, picks the right one, calls `kill(nonce)`.
///
/// Where: extracted in `NuSh::kill`; looks up the in-flight map and
/// SIGKILLs the holding worker via `kill_worker_pid`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct KillParams {
    pub nonce: String,
}

#[mcp::tool_router(router = kill_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Cancel an in-flight call by its nonce. SIGKILLs the worker holding the call; runs-pool workers are reaped and the next acquire spawns a fresh worker, interact respawn loses session state. Returns no payload; silently no-ops if the nonce is unknown or already completed (race-safe)."
    )]
    async fn kill(
        &self,
        mcp::Parameters(p): mcp::Parameters<KillParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let map = self.in_flight.lock().await;
        if let Some(entry) = map.get(&p.nonce) {
            let pid = entry.pid;
            drop(map);
            kill_worker_pid(pid);
        }
        Ok(mcp::CallToolResult::default())
    }
}
