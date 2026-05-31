use crate::*;

#[derive(Debug, Clone, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct Function {
    pub name: String,
    pub args_schema: String,
    pub result_schema: String,
    pub body: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RunParams {
    pub args_schema: String,
    pub result_schema: String,
    /// JSON object that becomes the nushell `$args` record literal at the
    /// __exec call site. Schemars represents `serde_json::Value` as the
    /// JSON Schema 2020-12 `true` keyword (match-anything), which Claude
    /// Code's MCP client rejects with "Invalid input" -- so we narrow the
    /// type to a Map (rmcp's `JsonObject` alias) which schemars renders
    /// as `{"type": "object"}`. Semantically correct anyway: args MUST be
    /// an object because it has to deserialize into a nushell record.
    pub args: mcp::JsonObject,
    #[serde(default)]
    pub functions: Vec<Function>,
    pub closure: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RerunParams {
    /// base62 rerun_id returned by a prior `run()` invocation. Names a
    /// `closures/<rerun_id>.json` cache file under
    /// `$XDG_CACHE_HOME/nu_sh_mcp/`.
    pub rerun_id: String,
    /// Per-call args. The args_schema baked into the cached closure
    /// gates this at parse time inside the worker.
    pub args: mcp::JsonObject,
}

/// On-disk shape of `closures/<rerun_id>.json`. Mirrors the relevant
/// subset of `RunParams` that uniquely identifies the closure.
/// Additive for future fields (`functions` lands here when the hash
/// grows to include them).
#[derive(Debug, ser::Deserialize, ser::Serialize)]
struct ClosureCacheBody {
    args_schema: String,
    result_schema: String,
    closure: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RegisterLibraryParams {
    /// Library name (top-level identifier). Becomes the directory name
    /// in the MCP repo under `$XDG_DATA_HOME/nu_sh_mcp/libraries/`.
    pub name: String,
    /// Client-side path where the MCP mirrors the library's files.
    /// Created if absent. Subsequent define_function calls write here
    /// alongside the MCP's canonical copy.
    pub path: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UnregisterLibraryParams {
    pub name: String,
}

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
    /// Schema for the function's `args` positional. Comma-separated
    /// `field: type` pairs (no surrounding `record<>`).
    pub args_schema: String,
    /// Schema for the function's return record.
    pub result_schema: String,
    /// Function body. Inlined inside `def main`'s block.
    pub body: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UndefineFunctionParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ImportLibraryParams {
    /// Library name to register under. Must be unused.
    pub name: String,
    /// Absolute path to the directory holding the pre-authored library
    /// source. The MCP validates the tree, copies it into the canonical
    /// repo, and records this path for future `reimport_library` calls.
    pub path: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ReimportLibraryParams {
    /// Library name. Must be already registered AND of kind=imported.
    pub name: String,
}

#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct CallParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
    /// JSON object passed as `$args` to the function. Schema match is
    /// enforced by the function's `main` signature at parse time inside
    /// the worker (typed positional binding on a literal record).
    pub args: mcp::JsonObject,
}

pub struct NuSh {
    runs_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    interact_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    nonce_gen: Arc<lib_empower::NonceGen>,
    library_locks: Arc<LibraryLocks>,
    #[allow(dead_code)]
    tool_router: mcp::ToolRouter<NuSh>,
}

#[mcp::tool_router]
impl NuSh {
    pub(crate) fn new(
        runs_worker: WorkerHandle,
        interact_worker: WorkerHandle,
        nonce_gen: Arc<lib_empower::NonceGen>,
        library_locks: Arc<LibraryLocks>,
    ) -> Self {
        Self {
            runs_worker: Arc::new(tk::AsyncMutex::new(runs_worker)),
            interact_worker: Arc::new(tk::AsyncMutex::new(interact_worker)),
            nonce_gen,
            library_locks,
            tool_router: Self::tool_router(),
        }
    }

    #[mcp::tool(
        description = "Evaluate a typed nushell closure on a stateless worker."
    )]
    async fn run(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<String, mcp::ErrorData> {
        let source = build_run_source(&p);
        let payload_bytes = json::to_vec(&p).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize RunParams for nonce: {e}"),
                None,
            )
        })?;
        let outcome = dispatch_to_worker(
            &self.runs_worker,
            &self.nonce_gen,
            CacheKind::Runs,
            &payload_bytes,
            source,
        )
        .await?;
        let rerun_id = lib_empower::RerunHash::of(&(
            p.args_schema.as_str(),
            p.result_schema.as_str(),
            p.closure.as_str(),
        ))
        .to_string();
        write_closure_cache(&rerun_id, &p)?;
        let envelope = json::json!({
            "result": outcome.result,
            "nonce": outcome.nonce.to_string(),
            "rerun_id": rerun_id,
        });
        json::to_string_json(&envelope).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("envelope serialize: {e}"),
                None,
            )
        })
    }

    #[mcp::tool(
        description = "Evaluate a typed administrative nushell closure on a persistent stateful worker."
    )]
    async fn interact(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<String, mcp::ErrorData> {
        let source = build_interact_source(&p);
        let payload_bytes = json::to_vec(&p).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize RunParams for nonce: {e}"),
                None,
            )
        })?;
        let outcome = dispatch_to_worker(
            &self.interact_worker,
            &self.nonce_gen,
            CacheKind::Interacts,
            &payload_bytes,
            source,
        )
        .await?;
        let envelope = json::json!({
            "result": outcome.result,
            "nonce": outcome.nonce.to_string(),
        });
        json::to_string_json(&envelope).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("envelope serialize: {e}"),
                None,
            )
        })
    }

    #[mcp::tool(
        description = "Register an empty library namespace; subsequent define_function calls populate it on both the MCP-managed canonical repo and the agent's local mirror at `path`."
    )]
    async fn register_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<RegisterLibraryParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self
            .library_locks
            .register(&p.name)
            .await
            .map_err(|_| {
                mcp::ErrorData::invalid_params(
                    format!("library `{}` is already registered", p.name),
                    None,
                )
            })?;
        let _guard = lock.write().await;
        register_library_impl(&p.name, std::path::Path::new(&p.path)).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("register_library: {e}"),
                None,
            )
        })?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Define (or replace) a single function inside a registered library. Writes `<library>/<module_path>/<name>.nu` with the `export def main` + `export def resolve` envelope, updates the `mod.nu` cascade up to the library root, mirrors to the agent's local copy, and commits."
    )]
    async fn define_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<DefineFunctionParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self.library_locks.lookup(&p.library).await.ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                format!("library `{}` is not registered", p.library),
                None,
            )
        })?;
        let _guard = lock.write().await;
        define_function_impl(
            &p.library,
            &p.module_path,
            &p.name,
            &p.args_schema,
            &p.result_schema,
            &p.body,
        )
        .map_err(|e| {
            mcp::ErrorData::internal_error(format!("define_function: {e}"), None)
        })?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Remove a function from a registered library. Updates the `mod.nu` cascade, prunes any now-empty intermediate directories, mirrors the removal, and commits."
    )]
    async fn undefine_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<UndefineFunctionParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self.library_locks.lookup(&p.library).await.ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                format!("library `{}` is not registered", p.library),
                None,
            )
        })?;
        let _guard = lock.write().await;
        undefine_function_impl(&p.library, &p.module_path, &p.name).map_err(|e| {
            mcp::ErrorData::internal_error(format!("undefine_function: {e}"), None)
        })?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Invoke a registered library function on a stateless worker. Builds `use <abs path to function file>.nu; <name> resolve (<name> $args)` so the function's `resolve` typecheck runs on the call's result. HEAD-only -- no version pinning."
    )]
    async fn call(
        &self,
        mcp::Parameters(p): mcp::Parameters<CallParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self.library_locks.lookup(&p.library).await.ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                format!("library `{}` is not registered", p.library),
                None,
            )
        })?;
        let _guard = lock.read().await;
        let file_path = call_file_path(&p.library, &p.module_path, &p.name).ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                "invalid library / module_path / name (must satisfy identifier rules)".to_string(),
                None,
            )
        })?;
        if !file_path.exists() {
            return Err(mcp::ErrorData::invalid_params(
                format!(
                    "function not defined: {}",
                    if p.module_path.is_empty() {
                        format!("{}/{}", p.library, p.name)
                    } else {
                        format!("{}/{}/{}", p.library, p.module_path, p.name)
                    },
                ),
                None,
            ));
        }
        let args_json = json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string());
        let source = format!(
            "use {}\n{} resolve ({} {})\n",
            file_path.display(),
            p.name,
            p.name,
            args_json,
        );
        let payload_bytes = json::to_vec(&p).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize CallParams for nonce: {e}"),
                None,
            )
        })?;
        let outcome = dispatch_to_worker(
            &self.runs_worker,
            &self.nonce_gen,
            CacheKind::Calls,
            &payload_bytes,
            source,
        )
        .await?;
        let envelope = json::json!({
            "result": outcome.result,
            "nonce": outcome.nonce.to_string(),
        });
        json::to_string_json(&envelope).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("envelope serialize: {e}"),
                None,
            )
        })
    }

    #[mcp::tool(
        description = "Import a pre-authored library from a client path into the MCP-managed canonical repo. Strict validation: each function file must have exactly `export def main [args: record<...>]` + `export def resolve [args: record<...>] { $args }`; each `mod.nu` may only re-export children. All violations are reported at once; no auto-fix."
    )]
    async fn import_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ImportLibraryParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self
            .library_locks
            .register(&p.name)
            .await
            .map_err(|_| {
                mcp::ErrorData::invalid_params(
                    format!("library `{}` is already registered", p.name),
                    None,
                )
            })?;
        let _guard = lock.write().await;
        import_library_impl(&p.name, std::path::Path::new(&p.path))
            .map_err(import_error_to_mcp_error)?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Re-import a library from the path it was originally imported from. Reads source_path from the library's metadata; re-runs strict validation; replaces the canonical copy with a fresh snapshot. Errors if the library was register_library-style (kind=registered) instead of import_library-style."
    )]
    async fn reimport_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ReimportLibraryParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self.library_locks.lookup(&p.name).await.ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                format!("library `{}` is not registered", p.name),
                None,
            )
        })?;
        let _guard = lock.write().await;
        reimport_library_impl(&p.name).map_err(import_error_to_mcp_error)?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Drop a library and all its functions from the MCP-managed canonical repo. Does not touch the agent's local mirror."
    )]
    async fn unregister_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<UnregisterLibraryParams>,
    ) -> Result<String, mcp::ErrorData> {
        let lock = self
            .library_locks
            .unregister(&p.name)
            .await
            .map_err(|_| {
                mcp::ErrorData::invalid_params(
                    format!("library `{}` is not registered", p.name),
                    None,
                )
            })?;
        let _guard = lock.write().await;
        unregister_library_impl(&p.name).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("unregister_library: {e}"),
                None,
            )
        })?;
        Ok(json::to_string_json(&json::json!({"ok": true})).expect("envelope serializes"))
    }

    #[mcp::tool(
        description = "Re-evaluate a cached stateless closure by rerun_id with new args."
    )]
    async fn rerun(
        &self,
        mcp::Parameters(p): mcp::Parameters<RerunParams>,
    ) -> Result<String, mcp::ErrorData> {
        if !lib_empower::is_base62(&p.rerun_id) {
            return Err(mcp::ErrorData::invalid_params(
                format!("rerun_id must be base62; got {:?}", p.rerun_id),
                None,
            ));
        }
        let path = closure_cache_file(&p.rerun_id);
        let cached_bytes = fs::read(&path).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("read {}: {e}", path.display()),
                None,
            )
        })?;
        let cached: ClosureCacheBody = json::from_slice(&cached_bytes).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("decode cached closure {}: {e}", path.display()),
                None,
            )
        })?;
        // Touch mtime for the LRU signal future pruning will use.
        // Idempotent overwrite -- content is deterministic.
        let _ = fs::write(&path, &cached_bytes);
        let reconstructed = RunParams {
            args_schema: cached.args_schema,
            result_schema: cached.result_schema,
            args: p.args,
            functions: Vec::new(),
            closure: cached.closure,
        };
        let source = build_run_source(&reconstructed);
        let payload_bytes = json::to_vec(&reconstructed).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize reconstructed RunParams for nonce: {e}"),
                None,
            )
        })?;
        let outcome = dispatch_to_worker(
            &self.runs_worker,
            &self.nonce_gen,
            CacheKind::Runs,
            &payload_bytes,
            source,
        )
        .await?;
        let envelope = json::json!({
            "result": outcome.result,
            "nonce": outcome.nonce.to_string(),
        });
        json::to_string_json(&envelope).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("envelope serialize: {e}"),
                None,
            )
        })
    }
}

/// Result of one worker round-trip plus its per-call log dir creation.
/// Each handler builds its own envelope on top because the envelope
/// shape varies (run includes rerun_id; interact + rerun don't).
struct DispatchOutcome {
    nonce: lib_empower::Nonce,
    result: json::Value,
}

async fn dispatch_to_worker(
    worker: &Arc<tk::AsyncMutex<WorkerHandle>>,
    nonce_gen: &lib_empower::NonceGen,
    log_kind: CacheKind,
    payload_for_nonce: &[u8],
    source: String,
) -> Result<DispatchOutcome, mcp::ErrorData> {
    let nonce = nonce_gen.next(&payload_for_nonce);
    let log_dir = cache_dir(log_kind, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("create_dir_all {}: {e}", log_dir.display()),
            None,
        )
    })?;
    let mut worker_guard = worker.lock().await;
    let response = worker_guard
        .send_request(log_dir, source)
        .await
        .map_err(|e| mcp::ErrorData::internal_error(e.to_string(), None))?;
    drop(worker_guard);
    if !response.ok {
        return Err(mcp::ErrorData::internal_error(
            response.error.unwrap_or_else(|| {
                "worker returned ok=false with no error".to_string()
            }),
            None,
        ));
    }
    let result: json::Value = msgpack::from_slice(&response.value)
        .unwrap_or(json::Value::Null);
    Ok(DispatchOutcome { nonce, result })
}

fn write_closure_cache(
    rerun_id: &str,
    p: &RunParams,
) -> Result<(), mcp::ErrorData> {
    let path = closure_cache_file(rerun_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("create_dir_all {}: {e}", parent.display()),
                None,
            )
        })?;
    }
    let body = ClosureCacheBody {
        args_schema: p.args_schema.clone(),
        result_schema: p.result_schema.clone(),
        closure: p.closure.clone(),
    };
    let bytes = json::to_vec(&body).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("serialize ClosureCacheBody: {e}"),
            None,
        )
    })?;
    fs::write(&path, &bytes).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("write {}: {e}", path.display()),
            None,
        )
    })?;
    Ok(())
}

fn import_error_to_mcp_error(e: ImportError) -> mcp::ErrorData {
    match e {
        ImportError::InvalidLibraryName(n) => mcp::ErrorData::invalid_params(
            format!("invalid library name: {n:?}"),
            None,
        ),
        ImportError::SourceMissing(p) => mcp::ErrorData::invalid_params(
            format!("source path does not exist or is not a directory: {}", p.display()),
            None,
        ),
        ImportError::NotRegistered(n) => mcp::ErrorData::invalid_params(
            format!("library `{n}` is not registered"),
            None,
        ),
        ImportError::WrongKind => mcp::ErrorData::invalid_params(
            "reimport_library only applies to libraries imported via import_library; this one was created via register_library".to_string(),
            None,
        ),
        ImportError::Violations(v) => mcp::ErrorData::invalid_params(
            format_violations(&v),
            None,
        ),
        ImportError::Io(e) => mcp::ErrorData::internal_error(
            format!("import: {e}"),
            None,
        ),
    }
}

fn format_violations(v: &[Violation]) -> String {
    let mut out = format!("validation failed: {} violation(s):", v.len());
    for vio in v {
        if vio.line == 0 {
            out.push_str(&format!("\n  - {}: {}", vio.path, vio.message));
        } else {
            out.push_str(&format!("\n  - {}:{}: {}", vio.path, vio.line, vio.message));
        }
    }
    out
}

#[mcp::tool_handler]
impl mcp::ServerHandler for NuSh {
    fn get_info(&self) -> mcp::ServerInfo {
        // Per [[rmcp-implementation-from-build-env-gotcha]]: must construct
        // Implementation via env! in THIS crate's source so the macros expand
        // against our CARGO_PKG_*, not rmcp's.
        //
        // ServerCapabilities::default() does NOT include `tools: Some(...)` --
        // empirically confirmed 2026-05-31 when Claude Code's MCP client
        // skipped tools/list after restart because the initialize response
        // advertised no tool capability. Explicit `.enable_tools()` is
        // required for the client to discover our tool surface.
        let mut info = mcp::ServerInfo::default();
        info.capabilities = mcp::ServerCapabilities::builder()
            .enable_tools()
            .build();
        info.server_info = mcp::Implementation::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        )
        .with_title("nushell");
        info.instructions = Some(
            "Evaluation artifacts are cached at \
             $XDG_CACHE_HOME/nu_sh_mcp/{runs,interacts,calls}/<nonce>/{stdout,stderr}; \
             closures cached at $XDG_CACHE_HOME/nu_sh_mcp/closures/<rerun_id>.json. \
             Registered libraries live in a signed git repo at \
             $XDG_DATA_HOME/nu_sh_mcp/libraries/; signing keypair at \
             $XDG_DATA_HOME/nu_sh_mcp/keypair/."
                .to_string(),
        );
        info
    }
}
