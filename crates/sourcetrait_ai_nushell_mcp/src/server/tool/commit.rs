use crate::*;

/// Parameters for `commit()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CommitParams {
    /// Name of the library to commit.
    pub library: String,
}

/// Success result of `commit()` -- the changed paths, grouped by kind.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CommitEnvelope {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

#[mcp::tool_router(router = commit_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Commit the agent's library source-code to the MCP's repository for live use.",
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
                added: result.added,
                modified: result.modified,
                removed: result.removed,
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
