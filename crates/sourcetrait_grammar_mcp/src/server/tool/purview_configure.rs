use crate::*;

/// Parameters for `purview_configure()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewConfigureParams {
    /// The purview to set, created if absent. A path-like snake label
    /// (`default`, `iter/almost`, `john/cindy/mary`) - always bare relative,
    /// never a leading `/` or `./`, and arbitrary rather than derived from any
    /// namepath or filesystem path.
    pub purview: String,
    /// The selectors it puts in view: namepath patterns, or an exact namepath
    /// for a single call. An EMPTY list DELETES the purview entirely.
    pub namepaths: Vec<String>,
}

/// Success result of `purview_configure()` - the state after the write.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewConfigureEnvelope {
    pub purviews: Vec<PurviewView>,
    pub current: Vec<PurviewView>,
    /// Selectors dropped because no registered library can satisfy them.
    pub pruned: Vec<String>,
}

#[mcp::tool_router(router = purview_configure_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Set a purview's selectors; an empty list deletes it.",
        output_schema = mcp::schema_for_type::<PurviewConfigureEnvelope>()
    )]
    pub(crate) async fn purview_configure(
        &self,
        mcp::Parameters(p): mcp::Parameters<PurviewConfigureParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        if is_derived_purview(&p.purview) {
            return Ok(error_to_call_result(
                Error::PurviewInvalidId {
                    id: p.purview.clone(),
                    reason: "`.` and `*` are derived, not stored, so they cannot be configured"
                        .to_string(),
                },
                None,
            ));
        }
        if !is_valid_purview_id(&p.purview) {
            return Ok(error_to_call_result(
                Error::PurviewInvalidId {
                    id: p.purview.clone(),
                    reason: "must be slash-separated snake components, bare relative \
                             (no leading `/` or `./`)"
                        .to_string(),
                },
                None,
            ));
        }
        // Configuring MATERIALIZES the file: from here on this namespace is
        // configured, and an absent `default` stays absent (and therefore
        // total) unless something explicitly writes it.
        let mut rows = match load_purviews() {
            Ok(rows) => rows.unwrap_or_default(),
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        rows.retain(|row| row.id != p.purview);
        if !p.namepaths.is_empty() {
            rows.push(PurviewRow {
                id: p.purview.clone(),
                namepath_patterns: p.namepaths.clone(),
            });
        }
        let pruned = prune_dangling(&mut rows);
        if let Err(error) = save_purviews(&rows) {
            return Ok(error_to_call_result(error, None));
        }
        // A deletion must not leave the session pointing at something gone.
        self.current_purview.retain_known(Some(&rows));
        let configured: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        envelope_to_structured(&PurviewConfigureEnvelope {
            purviews: purview_views(&configured, Some(&rows)),
            current: purview_views(&self.current_purview.ids(), Some(&rows)),
            pruned,
        })
    }
}
