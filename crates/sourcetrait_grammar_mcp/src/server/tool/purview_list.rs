use crate::*;

/// Parameters for `purview_list()` (none).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewListParams {}

/// Success result of `purview_list()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewListEnvelope {
    /// Every CONFIGURED purview, as persisted.
    pub purviews: Vec<PurviewView>,
    /// What is in view right now - the `.` built-in. Derived from the session
    /// rather than stored, and reported in the shape `info()` uses.
    pub current: Vec<PurviewView>,
}

#[mcp::tool_router(router = purview_list_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "List every configured purview, and what is currently in view.",
        output_schema = mcp::schema_for_type::<PurviewListEnvelope>()
    )]
    pub(crate) async fn purview_list(
        &self,
        mcp::Parameters(_p): mcp::Parameters<PurviewListParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        let configured: Vec<String> = rows
            .as_ref()
            .map(|r| r.iter().map(|row| row.id.clone()).collect())
            .unwrap_or_default();
        envelope_to_structured(&PurviewListEnvelope {
            purviews: purview_views(&configured, rows.as_ref()),
            current: purview_views(&self.current_purview.ids(), rows.as_ref()),
        })
    }
}
