use crate::*;

/// Parameters for `kill()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct KillParams {
    pub nonce: String,
}

#[mcp::tool_router(router = kill_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Cancel an in-flight usage by its nonce."
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
