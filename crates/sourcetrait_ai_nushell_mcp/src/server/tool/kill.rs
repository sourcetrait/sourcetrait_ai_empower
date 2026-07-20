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
        // the entry). A wedge (a pure-Rust loop that never polls Signals) cannot be
        // reached - the accepted residual; external children are reaped by the
        // teardown.
        if let Some(entry) = self.in_flight.lock().await.get(&p.nonce) {
            entry.cancel.store(true, Ordering::SeqCst);
        }
        Ok(mcp::CallToolResult::default())
    }
}
