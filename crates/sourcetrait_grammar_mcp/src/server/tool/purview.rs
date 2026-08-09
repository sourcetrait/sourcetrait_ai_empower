use crate::*;

/// Success result of `purview()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewSetEnvelope {
    /// `info()`'s `signatures` for what this call brought INTO view, or null.
    pub revealed: Option<String>,
}

/// Parameters for `purview()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewParams {
    /// The purview ids to put in view, REPLACING the current set.
    pub purviews: Vec<String>,
}

#[mcp::tool_router(router = purview_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Set the current view to these purviews.",
        output_schema = mcp::schema_for_type::<PurviewSetEnvelope>()
    )]
    pub(crate) async fn purview(
        &self,
        mcp::Parameters(p): mcp::Parameters<PurviewParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        let want: Vec<String> = p
            .purviews
            .iter()
            .map(|id| purview_ref(id).unwrap_or(id).to_string())
            .collect();
        for id in &want {
            if !is_nameable_purview(id, rows.as_ref()) {
                return Ok(error_to_call_result(
                    GrammarMcpError::PurviewInvalidId {
                        id: id.clone(),
                        reason: "no such purview; configure it first".to_string(),
                    },
                    None,
                ));
            }
        }
        let before = resolve_patterns(&self.current_purview.ids(), rows.as_ref());
        let ids = self.current_purview.set(&want);
        let after = resolve_patterns(&ids, rows.as_ref());
        let (revealed, _hidden) = pattern_delta(
            &expand_values(&before, rows.as_ref()),
            &expand_values(&after, rows.as_ref()),
        );
        let block = if revealed.is_empty() {
            None
        } else {
            Some(render_signatures_within(&self.rig_locks, &parse_patterns(&revealed)).await)
        };
        envelope_to_structured(&PurviewSetEnvelope { revealed: block })
    }
}
