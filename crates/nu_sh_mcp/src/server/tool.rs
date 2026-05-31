use crate::*;

/// What: agent-facing shape of one helper function passed as an
/// optional element of `RunParams.functions`. Carries the function
/// name, the args + result schemas, and the body.
///
/// Why: agents sometimes need helpers in scope inside a closure
/// (e.g. a shared transformation called from multiple expressions).
/// Allowing helpers as separate entries instead of forcing the agent
/// to inline the source into the closure body keeps the closure
/// itself focused on the call site logic.
///
/// Where: deserialized as part of `RunParams.functions` from the
/// agent's `run` / `interact` tool call; consumed by
/// `template::build_run_source` and `build_interact_source` which
/// emit one `def NAME [args: record<...>] { BODY }` per helper above
/// the `__exec` def.
#[derive(Debug, Clone, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct Function {
    pub name: String,
    pub args_schema: String,
    pub result_schema: String,
    pub body: String,
}

/// What: agent-facing parameters for `run()` and (because it has the
/// same shape) `interact()`. Carries the args + result schemas, the
/// JSON object that becomes `$args`, optional helper functions, and
/// the closure body that becomes `__exec`'s body.
///
/// Why: a single struct shared between run and interact keeps the
/// two tools' surface identical to the agent -- the only diff is
/// substrate (stateless vs stateful worker, do-block wrapping vs
/// not). schemars-derived JSON Schema makes the params visible to
/// Claude Code's tool selector.
///
/// Where: extracted via `mcp::Parameters<RunParams>` in the
/// `#[mcp::tool]` handlers `NuSh::run` and `NuSh::interact`; passed
/// to the corresponding template builders (`build_run_source`,
/// `build_interact_source`) and serialized into the nonce payload.
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

/// What: agent-facing parameters for `rerun()`. Carries the rerun_id
/// of a previously-cached closure plus fresh args for this
/// invocation.
///
/// Why: rerun lets an agent re-evaluate a known-good closure with
/// new args without re-sending the source -- saves tokens and pins
/// the body to the version that was originally tested. The
/// rerun_id is base62-validated before path-joining as defense in
/// depth against traversal.
///
/// Where: extracted via `mcp::Parameters<RerunParams>` in
/// `NuSh::rerun`; the rerun_id maps to a `closures/<rerun_id>.json`
/// cache file written by `write_closure_cache` during the original
/// run.
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

/// What: on-disk JSON shape of `closures/<rerun_id>.json`. Mirrors
/// the relevant subset of `RunParams` that uniquely identifies the
/// closure: schemas + body, no per-call args, no nonce, no helpers
/// (yet).
///
/// Why: rerun() needs enough to reconstruct a `RunParams` and
/// dispatch through the stateless worker; the schemas are the
/// typecheck inputs, the body is the executable surface. Additive
/// for future fields -- `functions` will land here when the rerun
/// hash grows to include them.
///
/// Where: serialized by `write_closure_cache` after a successful
/// `NuSh::run`; deserialized by `NuSh::rerun` via
/// `json::from_slice` to reconstruct a `RunParams` for replay.
#[derive(Debug, ser::Deserialize, ser::Serialize)]
struct ClosureCacheBody {
    args_schema: String,
    result_schema: String,
    closure: String,
}

/// What: agent-facing parameters for `register_library`. Carries the
/// library name + the client mirror path.
///
/// Why: registering creates an EMPTY library namespace (subsequent
/// `define_function` calls populate it); the agent supplies the
/// mirror path now so the MCP can write to both locations
/// atomically from then on.
///
/// Where: extracted in `NuSh::register_library`; passed to
/// `library::register_library_impl`. The lock registry is the entry
/// point of record (atomic check-and-insert via `register`).
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

/// What: agent-facing parameters for `unregister_library`. Just the
/// library name; the lock + meta + subtree are all keyed by it.
///
/// Why: unregister drops the library from the MCP repo + lock
/// registry. The client mirror is intentionally NOT touched per the
/// design call -- the client owns its copies after unregister.
///
/// Where: extracted in `NuSh::unregister_library`; passed to
/// `library::unregister_library_impl`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UnregisterLibraryParams {
    pub name: String,
}

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
    /// Schema for the function's `args` positional. Comma-separated
    /// `field: type` pairs (no surrounding `record<>`).
    pub args_schema: String,
    /// Schema for the function's return record.
    pub result_schema: String,
    /// Function body. Inlined inside `def main`'s block.
    pub body: String,
}

/// What: agent-facing parameters for `undefine_function`. Mirrors
/// `DefineFunctionParams`'s coordinate fields without the body and
/// schemas.
///
/// Why: undefine is purely a coordinate lookup -- the cached body
/// is on disk and gets removed; the cascade is updated; the commit
/// records the operation. No need for body/schemas on the wire.
///
/// Where: extracted in `NuSh::undefine_function`; passed to
/// `library::undefine_function_impl`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct UndefineFunctionParams {
    pub library: String,
    pub module_path: String,
    pub name: String,
}

/// What: agent-facing parameters for `import_library`. Carries the
/// library name plus the absolute path to a pre-authored library
/// source tree on the client.
///
/// Why: import is the agent-authored, MCP-validated path. The MCP
/// reads the source tree, runs the strict AST validator, then copies
/// it into the canonical repo. The recorded path is used by
/// `reimport_library` for re-snapshotting.
///
/// Where: extracted in `NuSh::import_library`; passed to
/// `library::import_library_impl`. The path is treated as immutable
/// metadata once written to `.nu_sh_mcp_meta.json`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ImportLibraryParams {
    /// Library name to register under. Must be unused.
    pub name: String,
    /// Absolute path to the directory holding the pre-authored library
    /// source. The MCP validates the tree, copies it into the canonical
    /// repo, and records this path for future `reimport_library` calls.
    pub path: String,
}

/// What: agent-facing parameters for `reimport_library`. Just the
/// library name; the original source path comes from the meta
/// sidecar.
///
/// Why: reimport re-reads from the path the agent originally
/// supplied to `import_library`, re-runs validation, replaces the
/// canonical copy. No new path parameter because mismatch with the
/// original would silently corrupt the registration.
///
/// Where: extracted in `NuSh::reimport_library`; passed to
/// `library::reimport_library_impl`. Errors if the library was
/// register-style instead of import-style.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ReimportLibraryParams {
    /// Library name. Must be already registered AND of kind=imported.
    pub name: String,
}

/// What: agent-facing parameters for `call`. Carries the
/// (library, module_path, name) coordinate of the function to invoke
/// plus the args object to pass as `$args`.
///
/// Why: call routes through the stateless worker with a synthesized
/// template `use <abs path>; <name> resolve (<name> ARGS_JSON)` so
/// the function's result_schema typecheck runs on every invocation.
/// HEAD-only -- no version pinning per the slice 3 lock.
///
/// Where: extracted in `NuSh::call`; the coordinate is path-validated
/// via `call_file_path`, the args become a record literal in the
/// synthesized source.
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

/// What: the rmcp server-side state. Owns Arc-wrapped handles to the
/// two worker subprocesses (stateless + stateful), the NonceGen for
/// per-call ids, the per-library lock registry for slice-3
/// concurrency, and rmcp's `ToolRouter` (populated by the
/// `#[mcp::tool_router]` macro).
///
/// Why: this is THE singleton the rmcp library serves. Every Arc
/// field is cheap to clone for concurrent tool calls; the workers
/// sit behind AsyncMutex because each worker is a single-request
/// channel; the library_locks sits behind its own internal
/// concurrency primitive.
///
/// Where: constructed in `server::run::run_server` after substrate
/// init and worker spawn; passed to `service.serve(stdio())` which
/// runs the MCP protocol against the host's stdin/stdout. Every
/// `#[mcp::tool]` method on this impl is a tool surface entry.
pub struct NuSh {
    runs_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    interact_worker: Arc<tk::AsyncMutex<WorkerHandle>>,
    nonce_gen: Arc<lib_empower::NonceGen>,
    library_locks: Arc<LibraryLocks>,
    lint_engine: Arc<ParseEngine>,
    #[allow(dead_code)]
    tool_router: mcp::ToolRouter<NuSh>,
}

#[mcp::tool_router]
impl NuSh {
    /// What: constructor for `NuSh`. Wraps the two `WorkerHandle`
    /// values in `Arc<tk::AsyncMutex<...>>` for shared serialized
    /// access, takes the already-Arc-wrapped nonce_gen + library_locks
    /// directly, and initializes the rmcp `tool_router` from the
    /// macro-generated `Self::tool_router()`.
    ///
    /// Why: all the wrapping happens here so calling code in
    /// `run_server` can pass plain `WorkerHandle`s and Arc references
    /// without juggling layers. The macro-generated tool_router has
    /// to be initialized AFTER all the inputs are owned, hence
    /// initialization at the bottom of the field block.
    ///
    /// Where: called once in `server::run::run_server` after
    /// substrate + worker spawn. Tests spawn their own NuSh
    /// indirectly via the host binary.
    pub(crate) fn new(
        runs_worker: WorkerHandle,
        interact_worker: WorkerHandle,
        nonce_gen: Arc<lib_empower::NonceGen>,
        library_locks: Arc<LibraryLocks>,
        lint_engine: Arc<ParseEngine>,
    ) -> Self {
        Self {
            runs_worker: Arc::new(tk::AsyncMutex::new(runs_worker)),
            interact_worker: Arc::new(tk::AsyncMutex::new(interact_worker)),
            nonce_gen,
            library_locks,
            lint_engine,
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
        // Slice 5.1: AST body lint runs BEFORE template synthesis so any
        // hardcoded-path or blacklisted-external violation surfaces as
        // -32602 invalid_params with the agent-fixable `lint::<class>
        // [L:C]` report shape. Helper functions in p.functions are NOT
        // linted in this slice -- that's slice 5.3.
        let violations = lint_body(&self.lint_engine, &p.args_schema, &p.closure, None);
        if !violations.is_empty() {
            return Err(mcp::ErrorData::invalid_params(
                format_lint_violations(&violations),
                None,
            ));
        }
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
        // Slice 5.2: lint applies to interact() bodies on the same shape
        // as run(); the stateful substrate is irrelevant for static lint.
        let violations = lint_body(&self.lint_engine, &p.args_schema, &p.closure, None);
        if !violations.is_empty() {
            return Err(mcp::ErrorData::invalid_params(
                format_lint_violations(&violations),
                None,
            ));
        }
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
        // Slice 5.2: lint the body BEFORE acquiring the lock or touching
        // disk; lint failure should be a fast client-side reject, not a
        // half-committed write.
        let violations = lint_body(&self.lint_engine, &p.args_schema, &p.body, None);
        if !violations.is_empty() {
            return Err(mcp::ErrorData::invalid_params(
                format_lint_violations(&violations),
                None,
            ));
        }
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
        import_library_impl(&p.name, std::path::Path::new(&p.path), &self.lint_engine)
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
        reimport_library_impl(&p.name, &self.lint_engine).map_err(import_error_to_mcp_error)?;
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

/// What: the shape returned by `dispatch_to_worker`. Pairs the
/// per-call `Nonce` with the decoded JSON `result` value.
///
/// Why: each handler (run/interact/rerun/call) builds its own
/// envelope on top of this because the envelope shape varies (run
/// includes rerun_id; interact + rerun + call don't; call has no
/// rerun caching). Returning a struct instead of a tuple makes the
/// per-field semantics readable at the call site.
///
/// Where: returned by `dispatch_to_worker` to each `#[mcp::tool]`
/// handler that wraps it into a json::json! envelope and serializes
/// to String.
struct DispatchOutcome {
    nonce: lib_empower::Nonce,
    result: json::Value,
}

/// What: shared worker round-trip path used by run/interact/rerun/
/// call. Computes the per-call nonce from the payload, creates the
/// per-call log dir under `$XDG_CACHE_HOME/nu_sh_mcp/<kind>/<nonce>/`,
/// acquires the worker mutex, sends the RunRequest, releases the
/// mutex, decodes the response, and returns the `DispatchOutcome`
/// (or an MCP-shaped error on any failure).
///
/// Why: the four handlers all need the same plumbing (nonce + log
/// dir + worker round-trip + error mapping); factoring it out keeps
/// the handlers small and forces consistent error mapping at the
/// rmcp seam. Releasing the worker mutex before computing the
/// envelope keeps the lock held for the shortest possible window.
///
/// Where: called by `NuSh::run`, `NuSh::interact`, `NuSh::rerun`,
/// and `NuSh::call`. Each handler picks the worker (`runs_worker`
/// or `interact_worker`) + `CacheKind` for its own semantics.
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

/// What: writes the closure cache file at `closures/<rerun_id>.json`
/// after a successful `run()`. Creates the parent dir if needed,
/// serializes the closure metadata (`args_schema`, `result_schema`,
/// `closure`) into `ClosureCacheBody`, and writes the JSON bytes.
/// Idempotent: the same rerun_id always produces the same bytes.
///
/// Why: rerun() needs a deterministic place to look up the cached
/// closure by its content-derived id. The unconditional overwrite
/// touches mtime even on identical content, which sets up future
/// LRU-style pruning.
///
/// Where: called by `NuSh::run` after `dispatch_to_worker` succeeds
/// and `RerunHash::of` produces the rerun_id. The matching read
/// happens in `NuSh::rerun` via `fs::read` + `json::from_slice`.
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

/// What: maps the typed `library::ImportError` enum variants onto
/// the rmcp `ErrorData` shapes the agent will see (invalid_params vs
/// internal_error, each with a human-readable message). `Violations`
/// variants are rendered as a multi-line report via
/// `format_violations`.
///
/// Why: keeping the typed `ImportError` internal lets the library
/// module be testable independent of rmcp; mapping at the seam
/// preserves the JSON-RPC error code semantics (-32602 for client
/// errors, -32603 for server errors).
///
/// Where: called by `NuSh::import_library` and `NuSh::reimport_library`
/// in their `.map_err(import_error_to_mcp_error)` chains.
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
        ImportError::Violations(result) => mcp::ErrorData::invalid_params(
            format_validation_result(&result),
            None,
        ),
        ImportError::Io(e) => mcp::ErrorData::internal_error(
            format!("import: {e}"),
            None,
        ),
    }
}

/// What: renders a slice of structural `Violation` records into a
/// multi-line human-readable report with one bullet per violation.
/// Lines with line=0 are file-level (no specific line); lines with
/// line>0 print `<path>:<line>: <message>`.
///
/// Why: structural import-validation findings have always rendered in
/// this bulleted-with-header shape; preserved verbatim so existing
/// agents and tests stay compatible.
///
/// Where: called by `format_validation_result` (slice 5.2) when the
/// structural section is non-empty.
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

/// What: renders a `ValidationResult` into a single multi-line message
/// combining the structural section (bulleted, via `format_violations`)
/// and the lint section (flat, via `format_lint_violations`), separated
/// by a blank line when both are present.
///
/// Why: slice 5.2 surfaces both kinds of findings from one
/// import_library / reimport_library call. The two render styles serve
/// different purposes -- structural errors describe shape problems
/// agents must restructure; lint findings name specific token-level
/// issues the agent must lift -- and keeping them in distinct
/// sections preserves both readers' workflows.
///
/// Where: called by `import_error_to_mcp_error` for the
/// `ImportError::Violations(ValidationResult)` arm.
fn format_validation_result(r: &ValidationResult) -> String {
    let mut out = String::new();
    if !r.structural.is_empty() {
        out.push_str(&format_violations(&r.structural));
    }
    if !r.lint.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format_lint_violations(&r.lint));
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
