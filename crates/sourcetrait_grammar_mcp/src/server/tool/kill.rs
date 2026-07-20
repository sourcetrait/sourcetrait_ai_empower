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
        // Cooperative cancel: flip the eval's interrupt Signals so it bails at
        // nushell's next check point + frees its permit. An unknown / already-
        // finished nonce is a race-safe no-op (its dispatch cleanup guard removes
        // the entry). A hung thread (a pure-Rust loop that never polls Signals)
        // cannot be reached - the accepted residual; external children are reaped
        // by the teardown.
        let found = {
            let map = self.in_flight.lock().await;
            match map.get(&p.nonce) {
                Some(entry) => {
                    entry.cancel.store(true, Ordering::SeqCst);
                    // Snapshot for the watchdog: a killed eval whose thread stays
                    // alive past grace is a confirmed hang (server/watchdog.rs).
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
        // For a real in-flight call the agent explicitly killed, escalate outside
        // the registry lock (blocking /proc walk + SIGKILL): reap its external tree
        // AND kill plugin subprocesses to unblock a plugin-hung call (a hung
        // plugin ignores the cancel Signals). An unknown / finished nonce is a pure
        // no-op.
        if let Some(pids) = found {
            tree_kill(&pids);
            kill_plugin_subprocesses();
        }
        Ok(mcp::CallToolResult::default())
    }
}
