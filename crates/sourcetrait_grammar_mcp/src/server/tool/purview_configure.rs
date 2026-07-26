use crate::*;

/// Parameters for `purview_configure()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct PurviewConfigureParams {
    /// The purview to set, created if absent; a path-like snake label.
    pub purview: String,
    /// Its namepath patterns; an EMPTY list DELETES the purview.
    pub namepaths: Vec<String>,
}

/// Success result of `purview_configure()` - the state after the write.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PurviewConfigureEnvelope {
    /// Everything this purview reveals; null when the call deleted it.
    pub signatures: Option<String>,
}

#[mcp::tool_router(router = purview_configure_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Set a purview's namepath patterns; an empty list deletes it.",
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
        for value in &p.namepaths {
            if let Some(id) = purview_ref(value)
                && !is_valid_purview_ref(id)
            {
                return Ok(error_to_call_result(
                    Error::PurviewInvalidId {
                        id: value.clone(),
                        reason: "`@` must reference a configurable purview; `@.` and `@*` \
                                 are derived and are never rows"
                            .to_string(),
                    },
                    None,
                ));
            }
        }
        let mut rows = match load_purviews() {
            Ok(rows) => rows.unwrap_or_default(),
            Err(error) => return Ok(error_to_call_result(error, None)),
        };
        rows.retain(|row| row.id != p.purview);
        let namepaths = if p.namepaths.is_empty() && p.purview == PURVIEW_DEFAULT {
            vec![PURVIEW_ALL.to_string()]
        } else {
            p.namepaths.clone()
        };
        if !namepaths.is_empty() {
            rows.push(PurviewRow {
                id: p.purview.clone(),
                namepath_patterns: namepaths,
            });
        }
        prune_dangling(&mut rows);
        if let Err(error) = save_purviews(&rows) {
            return Ok(error_to_call_result(error, None));
        }
        self.current_purview.retain_known(Some(&rows));
        let values = rows
            .iter()
            .find(|row| row.id == p.purview)
            .map(|row| row.namepath_patterns.clone());
        let signatures = match values {
            None => None,
            Some(values) => {
                let patterns = parse_patterns(&expand_values(&values, Some(&rows)));
                Some(render_signatures_within(&self.rig_locks, &patterns).await)
            }
        };
        envelope_to_structured(&PurviewConfigureEnvelope { signatures })
    }
}
