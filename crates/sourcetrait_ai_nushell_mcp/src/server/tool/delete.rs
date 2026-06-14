use crate::*;

/// Agent-facing parameters for `delete` - the guarded drop (leg 3).
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct DeleteParams {
    /// Library name to delete.
    pub library: String,
    /// The library's registered source_path, re-passed as a sanity check.
    /// Must equal the recorded path by PLAIN STRING EQUALITY (never
    /// canonicalized).
    pub source_path: String,
    /// When true, remove only the canonical (MCP) copy and leave the
    /// agent's source tree alone. Default false ALSO removes the source.
    #[serde(default)]
    pub mcp_only: bool,
}

/// One removed path in a delete() result: side is mcp | source.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RemovedEntry {
    pub path: String,
    pub side: String,
}

/// Success envelope for `delete`: the paths removed (idempotent - a side
/// already gone is simply omitted).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct DeleteEnvelope {
    pub removed: Vec<RemovedEntry>,
}

#[mcp::tool_router(router = delete_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Delete a library - the guarded full drop (rare by design). The source_path is re-passed and checked by plain string equality against the recorded path. Default ALSO removes the agent's source tree (symlink source_path -> unlink the link only; a real dir -> symlink-aware recursive removal); mcp_only=true removes only the canonical copy. Returns the removed paths {path, side: mcp|source}.",
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
