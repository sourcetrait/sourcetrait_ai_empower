use crate::*;

/// Parameters for `kill()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct KillParams {
    /// The nonce of the in-flight call to cancel; list them with processes().
    pub nonce: String,
}

#[mcp::tool_router(router = kill_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Cancel an in-flight usage by its nonce."
    )]
    pub(crate) async fn kill(
        &self,
        mcp::Parameters(p): mcp::Parameters<KillParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let found = {
            let map = self.in_flight.lock().await;
            match map.get(&p.nonce) {
                Some(entry) => {
                    entry.cancel.store(true, Ordering::SeqCst);
                    let lane = if matches!(entry.kind, InFlightKind::Interact) {
                        Lane::Interact
                    } else {
                        Lane::Stateless
                    };
                    register_hung(
                        &self.hung_watch,
                        HungWatch {
                            nonce: p.nonce.clone(),
                            tool: entry.tool,
                            lane,
                            started_at: entry.started_at,
                            cancelled_at: now_millis(),
                            finished: entry.finished.clone(),
                        },
                    );
                    Some(entry.tracker.collect_pids())
                }
                None => None,
            }
        };
        if let Some(pids) = found {
            tree_kill(&pids);
            kill_plugin_subprocesses();
        }
        Ok(mcp::CallToolResult::default())
    }
}
