use crate::*;

/// Parameters for `purview_extend()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewExtendParams {
    /// Purview ids to add to what is currently in view. Additive: nothing
    /// already in view is disturbed.
    pub purviews: Vec<String>,
}

/// What a change to the current view did, shared by extend and reset.
///
/// The two halves are asymmetric on purpose: extending normally only ADDS, and
/// resetting normally only REMOVES, so each tool reports null for the half it
/// did not touch rather than an empty list that reads like a result.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewDeltaEnvelope {
    /// `info()`'s `signatures` for what this call brought INTO view, or null.
    pub revealed: Option<String>,
    /// The purview ids in view now - the KEYS alone; `purviews()` says what
    /// each one resolves to.
    pub current: Vec<String>,
}

impl NuSh {
    /// The shared delta report for a change to the current view.
    ///
    /// `added` is `info()`'s `signatures` rendered for the namepath patterns
    /// that were newly added - NOT a diff of two whole blocks. So when the view
    /// already carries `*`, what it shows was visible before as well: the
    /// patterns are what changed, and the block says what they name.
    pub(crate) async fn purview_delta(
        &self,
        before: Vec<String>,
        after: Vec<String>,
        ids: Vec<String>,
        rows: Option<Vec<PurviewRow>>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // The DELTA is computed on the raw values, so `removed` reports what was
        // configured; only the block RENDER expands `@` references.
        // Both sides are EXPANDED before the delta. Comparing raw values would
        // count `@ants` as new against a `sourcetrait/ant:` already in view,
        // and reveal a block the caller could already see.
        let (revealed, _left) = pattern_delta(
            &expand_values(&before, rows.as_ref()),
            &expand_values(&after, rows.as_ref()),
        );
        let revealed_block = if revealed.is_empty() {
            None
        } else {
            Some(render_signatures_within(&self.rig_locks, &parse_patterns(&revealed)).await)
        };
        envelope_to_structured(&PurviewDeltaEnvelope {
            revealed: revealed_block,
            current: ids,
        })
    }
}

#[mcp::tool_router(router = purview_extend_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Bring more purviews into the current view.",
        output_schema = mcp::schema_for_type::<PurviewDeltaEnvelope>()
    )]
    pub(crate) async fn purview_extend(
        &self,
        mcp::Parameters(p): mcp::Parameters<PurviewExtendParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let rows = match load_purviews() {
            Ok(rows) => rows,
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        // An unknown id is REFUSED rather than ignored. Contributing nothing in
        // silence would turn a typo into a view that simply fails to widen,
        // which looks identical to a purview that is genuinely empty.
        for id in &p.purviews {
            if !is_nameable_purview(id, rows.as_ref()) {
                return Ok(error_to_call_result(
                    Error::PurviewInvalidId {
                        id: id.clone(),
                        reason: "no such purview; configure it first".to_string(),
                    },
                    None,
                ));
            }
        }
        let before = resolve_patterns(&self.current_purview.ids(), rows.as_ref());
        let ids = self.current_purview.extend(&p.purviews);
        let after = resolve_patterns(&ids, rows.as_ref());
        self.purview_delta(before, after, ids, rows).await
    }
}
