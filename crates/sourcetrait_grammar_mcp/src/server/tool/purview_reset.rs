use crate::*;

/// Parameters for `purview_reset()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewResetParams {}

#[mcp::tool_router(router = purview_reset_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Reset the current view back to the default purview.",
        output_schema = mcp::schema_for_type::<PurviewDeltaEnvelope>()
    )]
    pub(crate) async fn purview_reset(
        &self,
        mcp::Parameters(_p): mcp::Parameters<PurviewResetParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        let before = resolve_patterns(&self.current_purview.ids(), rows.as_ref());
        // Back to the startup state - `default`, which is everything when the
        // namespace has never been configured.
        let ids = self.current_purview.reset();
        let after = resolve_patterns(&ids, rows.as_ref());
        self.purview_delta(before, after, ids, rows).await
    }
}
