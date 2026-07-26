use crate::*;

/// Parameters for `purviews()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewsParams {}

/// Success result of `purviews()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewsEnvelope {
    /// Every CONFIGURED purview, as persisted.
    pub purviews: Vec<PurviewView>,
    /// The purview ids in view right now - the KEYS alone.
    pub current: Vec<String>,
}

#[mcp::tool_router(router = purviews_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "List every configured purview, and what is currently in view.",
        output_schema = mcp::schema_for_type::<PurviewsEnvelope>()
    )]
    pub(crate) async fn purviews(
        &self,
        mcp::Parameters(_p): mcp::Parameters<PurviewsParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        let configured: Vec<String> = rows
            .as_ref()
            .map(|r| r.iter().map(|row| row.id.clone()).collect())
            .unwrap_or_default();
        envelope_to_structured(&PurviewsEnvelope {
            purviews: purview_views(&configured, rows.as_ref()),
            current: self.current_purview.ids(),
        })
    }
}
