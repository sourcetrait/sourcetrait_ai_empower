use crate::*;

/// Parameters for `delete()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct DeleteParams {
    /// Library name to delete.
    pub library: String,
    /// The library's source_path, re-passed as a confirmation check.
    pub source_path: String,
    /// When true, remove only the MCP's copy and leave the agent's source tree (default also removes the source).
    #[serde(default)]
    pub mcp_only: bool,
}

/// One removed path in a `delete()` result.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RemovedEntry {
    pub path: String,
    pub side: String,
}

/// Success result of `delete()` -- the removed paths.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct DeleteEnvelope {
    pub removed: Vec<RemovedEntry>,
}

#[mcp::tool_router(router = delete_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Delete a library.",
        output_schema = mcp::schema_for_type::<DeleteEnvelope>()
    )]
    async fn delete(
        &self,
        mcp::Parameters(p): mcp::Parameters<DeleteParams>,
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
        match delete_impl(&p.library, &p.source_path, p.mcp_only) {
            Ok(result) => envelope_to_structured(&DeleteEnvelope {
                removed: result
                    .removed
                    .into_iter()
                    .map(|r| RemovedEntry {
                        path: r.path,
                        side: r.side,
                    })
                    .collect(),
            }),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
