use crate::*;

/// What: agent-facing parameters for `define_function`. Carries the
/// (library, module_path, name) coordinate plus the args + result
/// schemas and the function body.
///
/// Why: granular per-function write is the post-MTP slice 1 design
/// (one commit per define, one commit per undefine, fully
/// addressable cascade updates). All three identifier components are
/// validated via `is_valid_ident`/`is_valid_module_path` before any
/// filesystem op.
///
/// Where: extracted in `NuSh::define_function`; passed to
/// `library::define_function_impl` which synthesizes the function
/// file, updates the mod.nu cascade, mirrors to client (if
/// registered), and commits.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct DefineFunctionParams {
    /// Library name (must be already-registered).
    pub library: String,
    /// Slash-separated module path within the library. Empty for a
    /// function at the library root. Each segment must satisfy the
    /// same identifier shape as `name`.
    pub module_path: String,
    /// Function name; becomes the filename `<name>.nu`. Identifier
    /// shape `[a-zA-Z_][a-zA-Z0-9_-]*`; `mod` reserved.
    pub name: String,
    /// Structured args schema: a JSON object of `field -> type` (item
    /// 21 grammar). `{}` means a no-args (void) function.
    pub args_schema: mcp::JsonObject,
    /// Structured result schema: a JSON object of `field -> type`. `{}`
    /// means a void return.
    pub result_schema: mcp::JsonObject,
    /// Function body. Inlined inside `def main`'s block.
    pub body: String,
}

#[mcp::tool_router(router = define_function_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Define (or replace) a single function inside a registered library. Writes `<library>/<module_path>/<name>.nu` with the `export def main` + `export def resolve` envelope, updates the `mod.nu` cascade up to the library root, mirrors to the agent's local copy, and commits.",
        output_schema = mcp::schema_for_type::<ErrorEnvelope>()
    )]
    async fn define_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<DefineFunctionParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let (args_type, result_type) =
            match convert_schemas(&p.args_schema, &p.result_schema) {
                Ok(t) => t,
                Err(reason) => return Ok(error_to_call_result(
                    Error::SchemaInvalid { reason },
                    None,
                )),
            };
        let violations = lint_body(&self.lint_engine, &args_type, &p.body, None);
        if !violations.is_empty() {
            return Ok(error_to_call_result(
                Error::LintViolations { violations },
                None,
            ));
        }
        let parse_violations = parse_check_function_source(
            &self.lint_engine,
            &p.name,
            &args_type,
            &result_type,
            &p.body,
        );
        if !parse_violations.is_empty() {
            return Ok(error_to_call_result(
                Error::LibraryViolations {
                    structural: parse_violations,
                    lint: vec![],
                },
                None,
            ));
        }
        let lock = match self.library_locks.lookup(&p.library).await {
            Some(l) => l,
            None => return Ok(error_to_call_result(
                Error::LibraryNotRegistered { library: p.library.clone() },
                None,
            )),
        };
        let _guard = lock.write().await;
        match define_function_impl(
            &p.library,
            &p.module_path,
            &p.name,
            &args_type,
            &result_type,
            &p.body,
        ) {
            Ok(()) => Ok(mcp::CallToolResult::default()),
            Err(error) => Ok(error_to_call_result(error, None)),
        }
    }
}
