use crate::*;

/// What: the minimized context-threshold artifact - the session nom that
/// names the file plus exactly the context-window counts the agent needs to
/// compute and project window usage.
///
/// Why: the full status payload is far more than a per-tick threshold check
/// needs; this typed subset is the agent's cheap, stable read
/// (context/<nom>.yaml + latest.yaml). used_percentage is intentionally
/// absent - the agent derives an exact percent from total_input_tokens /
/// context_window_size and projects with total_output_tokens.
///
/// Where: produced by `TryFrom<Input>` in run() after the status mirror is
/// written; serialized to YAML by store's persist_context.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ContextModel {
    pub(crate) session_nom: String,
    pub(crate) context_window: ContextWindow,
}

/// What: the three context-window token counts the threshold calc rides on.
///
/// Why: total_input_tokens / context_window_size is exact current usage and
/// total_output_tokens feeds the next-tick projection. These three define
/// the upstream schema claudeline depends on - if any cannot be read, the
/// conversion fails and the canary fires.
///
/// Where: the sole nested field of ContextModel.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ContextWindow {
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) context_window_size: i64,
}

/// What: zero-data marker - the statusline JSON did not carry the
/// context_window fields ContextModel depends on (a renamed field, a renamed
/// container, or an outright drop: an upstream schema change).
///
/// Why: turns a missing field into the degraded canary artifact
/// (`error: statusline JSON schema has changed`) the agent can see, rather
/// than a silently wrong or empty context file.
///
/// Where: the Err half of `TryFrom<Input> for ContextModel`; mapped to the
/// degraded write by store's persist_context.
#[derive(Debug)]
pub(crate) struct ContextSchemaChanged;

impl TryFrom<Input> for ContextModel {
    type Error = ContextSchemaChanged;

    /// What: consume the parsed payload and extract the context-threshold
    /// subset; Err(ContextSchemaChanged) when the depended-on context_window
    /// fields are absent or non-integer.
    ///
    /// Why: takes Input by value because the status mirror has already been
    /// written by the time this runs - the payload is spent, so the kept
    /// fields move into the typed model with nothing left to borrow. The
    /// fallible path is the schema-change canary.
    ///
    /// Where: run(), immediately after persist_status; the Ok/Err is handed
    /// to store's persist_context.
    fn try_from(input: Input) -> Result<Self, Self::Error> {
        let sid = input.session_id().ok_or(ContextSchemaChanged)?;
        let session_nom = ClaudeSessionNom::from(sid).to_string();
        let cw = input
            .value
            .get("context_window")
            .ok_or(ContextSchemaChanged)?;
        Ok(Self {
            session_nom,
            context_window: ContextWindow {
                total_input_tokens: cw_int(cw, "total_input_tokens")?,
                total_output_tokens: cw_int(cw, "total_output_tokens")?,
                context_window_size: cw_int(cw, "context_window_size")?,
            },
        })
    }
}

/// What: read an integer field out of the context_window object, or
/// Err(ContextSchemaChanged) when it is missing or not an integer.
///
/// Why: the three counts share one extraction rule; a named fn (not a
/// captured closure) keeps it inline-callable without capturing `cw`.
///
/// Where: TryFrom<Input> for ContextModel, once per kept field.
fn cw_int(
    cw: &serde_json::Value,
    key: &str,
) -> Result<i64, ContextSchemaChanged> {
    cw.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or(ContextSchemaChanged)
}
