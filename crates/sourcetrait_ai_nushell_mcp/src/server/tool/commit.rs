use crate::*;

/// Agent-facing parameters for `commit` - the validate-and-promote
/// upsert (leg 3). Library NAME only; source_path is read from the meta.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CommitParams {
    /// Library name to commit. Its source tree is re-read from the
    /// source_path recorded at new() establishment, validated, and
    /// upserted into the canonical signed repo.
    pub library: String,
}

/// One changed path in a commit() result: kind is added | modified | removed.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ChangedEntry {
    pub path: String,
    pub kind: String,
}

/// Success envelope for `commit`: the paths changed by this upsert
/// (empty = idempotent no-op, the source already matched the canonical).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CommitEnvelope {
    pub changed: Vec<ChangedEntry>,
}

#[mcp::tool_router(router = commit_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Validate the library's source tree (read from the source_path it was established with) and upsert it into the canonical signed repo - the central edit -> commit -> call iterate step. Idempotent on a no-change resync; returns the changed paths {path, kind: added|modified|removed}.",
        output_schema = mcp::schema_for_type::<CommitEnvelope>()
    )]
    async fn commit(
        &self,
        mcp::Parameters(p): mcp::Parameters<CommitParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = match self.library_locks.lookup(&p.library).await {
            Some(l) => l,
            None => {
                return Ok(error_to_call_result(
                    Error::LibraryNotRegistered {
                        library: p.library.clone(),
                    },
                    None,
                ));
            }
        };
        let _guard = lock.write().await;
        match commit_impl(&p.library, &self.lint_engine) {
            Ok(result) => envelope_to_structured(&CommitEnvelope {
                changed: result
                    .changed
                    .into_iter()
                    .map(|c| ChangedEntry {
                        path: c.path,
                        kind: c.kind,
                    })
                    .collect(),
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
