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
        // Phase 1 (in-process, pre-cancellation): no per-eval process exists to
        // SIGKILL, so kill is a race-safe no-op - the in-flight entry is removed by
        // its own dispatch cleanup guard. Phase 2 re-points this to trigger the
        // eval's Signals for real cooperative cancellation.
        let _ = &p.nonce;
        Ok(mcp::CallToolResult::default())
    }
}
