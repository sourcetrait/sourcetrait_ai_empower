use crate::*;

/// What: agent-facing parameters for `run()` and (because it has the
/// same shape) `interact()`. Carries the args + result schemas, the
/// JSON object that becomes `$args`, and the body that becomes the
/// agent's submission.
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
    pub body: String,
    /// Optional per-call timeout in milliseconds. When the worker
    /// round-trip exceeds this, the call returns -32001 and the worker
    /// is killed (runs-pool worker is reaped, interact worker is
    /// respawned losing session state). Defaults to 120000 (2 minutes)
    /// when omitted. No upper cap -- agent picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
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
    /// Optional per-call timeout in milliseconds. Same semantics as
    /// `RunParams.timeout_ms`. Defaults to 120000 when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// What: agent-facing success envelope for `run()`. Carries the
/// closure's typed return value (per `result_schema`), the per-call
/// nonce, and the content-derived `rerun_id` when caching succeeded.
///
/// Why: a typed struct (rather than ad-hoc `serde_json::json!`)
/// gives schemars an outputSchema to publish on `run`'s tool
/// descriptor, lets rmcp emit the value via `structured_content`,
/// and lets future-me reason about the envelope by name rather than
/// by JSON key lookup.
///
/// Where: returned from `NuSh::run` wrapped in a `CallToolResult`
/// whose `structured_content` field carries the serialized
/// envelope. The matching `outputSchema` is declared on the
/// `#[mcp::tool]` attribute via `schema_for_type::<RunEnvelope>()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RunEnvelope {
    /// Worker-evaluated return value of the body. Always a JSON object
    /// because the worker template runs `__resolve [result: record<...>]
    /// $result` which forces a record-shaped return. `mcp::JsonObject`
    /// (not `serde_json::Value`) keeps the schemars rendering as
    /// `{"type": "object"}` -- the `true` rendering Value would produce
    /// is rejected by Claude Code's MCP client schema validator (same
    /// gotcha as `RunParams.args`).
    pub result: mcp::JsonObject,
    pub nonce: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_id: Option<String>,
}

/// Success envelope for `interact()`. Same shape as `RunEnvelope`
/// minus `rerun_id` -- interact() does not cache closures (stateful
/// session bodies don't make sense to replay independently).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InteractEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

/// Success envelope for `call()`. Carries the registered library
/// function's typed return value plus the per-call nonce. No
/// `rerun_id` (library calls are themselves the replayable unit;
/// recipe is `call(library, module_path, name, args)`).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CallEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

/// Success envelope for `rerun()`. The agent supplied the rerun_id
/// so we don't echo it back; just the result + a fresh nonce.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RerunEnvelope {
    pub result: mcp::JsonObject,
    pub nonce: String,
}

/// Per-tool-call entry returned in the `processes()` snapshot.
/// `args` is always object-shaped (matches the agent's submission
/// shape for run/interact/call/rerun). `rerun_id` populated only
/// for rerun calls; `path` only for call calls.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ProcessEntry {
    pub nonce: String,
    pub tool: String,
    pub started_at: u64,
    pub args: mcp::JsonObject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Success envelope for `processes()`. The snapshot field carries
/// zero or more `ProcessEntry` records, one per in-flight tool
/// call.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct ProcessesEnvelope {
    pub processes: Vec<ProcessEntry>,
}

/// What: on-disk JSON shape of `closures/<rerun_id>.json`. Mirrors
/// the relevant subset of `RunParams` that uniquely identifies the
/// closure: schemas + body, no per-call args, no nonce.
///
/// Why: rerun() needs enough to reconstruct a `RunParams` and
/// dispatch through the stateless worker; the schemas are the
/// typecheck inputs, the body is the executable surface.
///
/// Where: serialized by `write_closure_cache` after a successful
/// `NuSh::run`; deserialized by `NuSh::rerun` via
/// `json::from_slice` to reconstruct a `RunParams` for replay.
#[derive(Debug, ser::Deserialize, ser::Serialize)]
struct ClosureCacheBody {
    args_schema: String,
    result_schema: String,
    body: String,
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
    /// Optional per-call timeout in milliseconds. Same semantics as
    /// `RunParams.timeout_ms`. Defaults to 120000 when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// What: agent-facing parameters for `kill`. The `nonce` is the value
/// returned in a prior tool envelope (the per-call id rendered as a
/// base62 string).
///
/// Why: cancellation needs to address one specific in-flight call;
/// `nonce` is the existing per-call id we already give the agent in
/// every envelope, so no new identifier is needed. Agent uses
/// `processes()` to discover live nonces, matches against their own
/// send-set via `args`, picks the right one, calls `kill(nonce)`.
///
/// Where: extracted in `NuSh::kill`; looks up the in-flight map and
/// SIGKILLs the holding worker via `kill_worker_pid`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct KillParams {
    pub nonce: String,
}

/// What: agent-facing parameters for `processes`. Empty -- the tool
/// takes no input. Returns a snapshot of every in-flight call on the
/// host.
///
/// Why: an empty params struct (`{}`) is the schemars-friendly shape
/// rmcp expects for a no-arg tool; not having any params keeps the
/// tool surface explicit.
///
/// Where: extracted in `NuSh::processes`; the body just snapshots the
/// in-flight map and serializes per-tool entry shapes.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct ProcessesParams {}

/// What: agent-facing parameters for `info`. Empty -- info takes no
/// input.
///
/// Why: `info()` surfaces static server state for agent
/// introspection; nothing per-call to parameterize.
///
/// Where: extracted in `NuSh::info`; the handler builds the envelope
/// from `env!` macros (name + version + nu_version) and a host-side
/// plugin enumeration.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct InfoParams {}

/// What: agent-facing success envelope for `info`. Carries the
/// crate name + version, the embedded nushell engine version
/// captured at build time, and the list of plugin names + versions
/// visible to the worker's registry.
///
/// Why: lifts handshake-only `serverInfo` to the tool surface so
/// agents can read it via `tools/call` instead of relying on the
/// rmcp client to relay handshake metadata. Plugin list is
/// enumerated from the same `plugin.msgpackz` registry the worker
/// loads from at startup, so the agent's view matches the worker's.
///
/// Where: returned from `NuSh::info` wrapped in a `CallToolResult`
/// whose `structured_content` carries the serialized envelope.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InfoEnvelope {
    pub name: String,
    pub version: String,
    pub nu_version: String,
    pub plugins: Vec<crate::plugins::PluginInfo>,
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
    runs_pool: Arc<Pool>,
    interact_worker: Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
    nonce_gen: Arc<lib_empower::NonceGen>,
    library_locks: Arc<LibraryLocks>,
    lint_engine: Arc<ParseEngine>,
    in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    #[allow(dead_code)]
    tool_router: mcp::ToolRouter<NuSh>,
}

/// Default per-call timeout when `timeout_ms` is omitted (the_user
/// 2026-06-01: 120s catches hangs without imposing an upper cap).
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// JSON-RPC error code returned when a call exceeds `timeout_ms`
/// (the_user 2026-06-01: `-32001` chosen from the JSON-RPC
/// implementation-defined server-error range, distinct from -32603
/// internal_error so agents can branch).
const TIMEOUT_ERROR_CODE: i32 = -32001;

/// What: snapshot of one in-flight tool call. Lives in `NuSh::in_flight`
/// keyed by the call's nonce string; `processes()` serializes it,
/// `kill(nonce)` looks up the worker pid through it.
///
/// Why: identification + cancellation (slice 5.9) requires the host to
/// remember which worker process is handling which logical call. `args`
/// is the agent-supplied data the agent uses to match entries against
/// their own send-set (the_user 2026-06-01: "if it's doing things
/// correctly, args should always be different").
///
/// Where: inserted at the top of each dispatch (run/interact/rerun/
/// call), removed via `InFlightGuard::drop` when dispatch returns.
/// Read by `NuSh::processes` and `NuSh::kill`.
pub(crate) struct InFlightEntry {
    pub tool: &'static str,
    pub started_at: u64,
    pub args: serde_json::Value,
    pub pid: u32,
    pub kind: InFlightKind,
}

/// Tool-specific extra fields per the_user 2026-06-01 per-tool entry
/// shape lock: run/interact have nothing extra; rerun carries the
/// `rerun_id` it was invoked with; call carries the
/// `library:module/path:name` flat string.
pub(crate) enum InFlightKind {
    Run,
    Interact,
    Rerun { rerun_id: String },
    Call { path: String },
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
        runs_pool: Arc<Pool>,
        interact_worker: WorkerHandle,
        nonce_gen: Arc<lib_empower::NonceGen>,
        library_locks: Arc<LibraryLocks>,
        lint_engine: Arc<ParseEngine>,
    ) -> Self {
        Self {
            runs_pool,
            interact_worker: Arc::new(tk::AsyncMutex::new(Some(interact_worker))),
            nonce_gen,
            library_locks,
            lint_engine,
            in_flight: Arc::new(tk::AsyncMutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }

    #[mcp::tool(
        description = "Evaluate a typed nushell closure on a stateless worker.",
        output_schema = mcp::schema_for_type::<RunEnvelope>()
    )]
    async fn run(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Slice 5.1: AST body lint runs BEFORE template synthesis so any
        // hardcoded-path or denied-external violation surfaces as -32602
        // invalid_params with the agent-fixable `lint::<class> [L:C]`
        // report shape.
        let violations = lint_run_params(&self.lint_engine, &p);
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
        let args_json = serde_json::Value::Object(p.args.clone());
        let timeout_ms = p.timeout_ms;
        let outcome = dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Runs,
            &payload_bytes,
            source,
            "run",
            args_json,
            InFlightKind::Run,
            timeout_ms,
        )
        .await?;
        let computed_rerun_id = lib_empower::RerunHash::of(&(
            p.args_schema.as_str(),
            p.result_schema.as_str(),
            p.body.as_str(),
        ))
        .to_string();
        // Slice 6.0: cache write is non-fatal. A successful eval whose
        // closure couldn't be cached (disk full, permission, etc.) still
        // returns the result to the agent; the envelope omits `rerun_id`
        // so the agent knows replay is unavailable. Loss is logged to
        // host stderr for observability.
        let rerun_id_opt = match write_closure_cache(&computed_rerun_id, &p) {
            Ok(()) => Some(computed_rerun_id),
            Err(e) => {
                eprintln!(
                    "nu_sh_mcp: write_closure_cache failed for {computed_rerun_id}: {e}",
                );
                None
            }
        };
        // outcome.result is always a JSON object because the worker's
        // `__resolve [result: record<RESULT_SCHEMA>] { $result }` binding
        // forces a record-shaped return. If the typed binding ever fails,
        // the worker emits ok=false with the cant_convert error and we
        // bail before this point; here we trust the shape.
        let result_obj = outcome
            .result
            .as_object()
            .cloned()
            .unwrap_or_default();
        let envelope = RunEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
            rerun_id: rerun_id_opt,
        };
        envelope_to_structured(&envelope)
    }

    #[mcp::tool(
        description = "Evaluate a typed administrative nushell closure on a persistent stateful worker.",
        output_schema = mcp::schema_for_type::<InteractEnvelope>()
    )]
    async fn interact(
        &self,
        mcp::Parameters(p): mcp::Parameters<RunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        // Slice 5.2: lint applies to interact() bodies on the same
        // shape as run(); the stateful substrate is irrelevant for
        // static lint, so both paths share the same `lint_run_params`
        // aggregator.
        let violations = lint_run_params(&self.lint_engine, &p);
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
        let args_json = serde_json::Value::Object(p.args.clone());
        let timeout_ms = p.timeout_ms;
        let outcome = dispatch_interact(
            &self.interact_worker,
            &self.nonce_gen,
            &self.in_flight,
            &payload_bytes,
            source,
            args_json,
            timeout_ms,
        )
        .await?;
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        let envelope = InteractEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        };
        envelope_to_structured(&envelope)
    }

    #[mcp::tool(
        description = "Register an empty library namespace; subsequent define_function calls populate it on both the MCP-managed canonical repo and the agent's local mirror at `path`."
    )]
    async fn register_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<RegisterLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Define (or replace) a single function inside a registered library. Writes `<library>/<module_path>/<name>.nu` with the `export def main` + `export def resolve` envelope, updates the `mod.nu` cascade up to the library root, mirrors to the agent's local copy, and commits."
    )]
    async fn define_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<DefineFunctionParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        // Slice 6.0: parse-check the synthesized source BEFORE acquiring
        // the lock or touching disk. lint_body returns no violations when
        // the wrapper parse fails (find_def_body_id early-return), so a
        // syntactically broken body would otherwise be committed and only
        // surface at call() time. Mirrors the parse-correctness check
        // import_library already performs via validate_function_file_ast.
        let parse_violations = parse_check_function_source(
            &self.lint_engine,
            &p.name,
            &p.args_schema,
            &p.result_schema,
            &p.body,
        );
        if !parse_violations.is_empty() {
            return Err(mcp::ErrorData::invalid_params(
                format_violations(&parse_violations),
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
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Remove a function from a registered library. Updates the `mod.nu` cascade, prunes any now-empty intermediate directories, mirrors the removal, and commits."
    )]
    async fn undefine_function(
        &self,
        mcp::Parameters(p): mcp::Parameters<UndefineFunctionParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Invoke a registered library function on a stateless worker. Builds `use <abs path to function file>.nu; <name> resolve (<name> $args)` so the function's `resolve` typecheck runs on the call's result. HEAD-only -- no version pinning.",
        output_schema = mcp::schema_for_type::<CallEnvelope>()
    )]
    async fn call(
        &self,
        mcp::Parameters(p): mcp::Parameters<CallParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        let args_json_str = json::to_string_json(&p.args).unwrap_or_else(|_| "{}".to_string());
        let source = format!(
            "use {}\n{} resolve ({} {})\n",
            file_path.display(),
            p.name,
            p.name,
            args_json_str,
        );
        let payload_bytes = json::to_vec(&p).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize CallParams for nonce: {e}"),
                None,
            )
        })?;
        let path_str = if p.module_path.is_empty() {
            format!("{}::{}", p.library, p.name)
        } else {
            format!("{}:{}:{}", p.library, p.module_path, p.name)
        };
        let args_json = serde_json::Value::Object(p.args.clone());
        let outcome = dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Calls,
            &payload_bytes,
            source,
            "call",
            args_json,
            InFlightKind::Call { path: path_str },
            p.timeout_ms,
        )
        .await?;
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        envelope_to_structured(&CallEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        })
    }

    #[mcp::tool(
        description = "Import a pre-authored library from a client path into the MCP-managed canonical repo. Strict validation: each function file must have exactly `export def main [args: record<...>]` + `export def resolve [args: record<...>] { $args }`; each `mod.nu` may only re-export children. All violations are reported at once; no auto-fix."
    )]
    async fn import_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ImportLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Re-import a library from the path it was originally imported from. Reads source_path from the library's metadata; re-runs strict validation; replaces the canonical copy with a fresh snapshot. Errors if the library was register_library-style (kind=registered) instead of import_library-style."
    )]
    async fn reimport_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<ReimportLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let lock = self.library_locks.lookup(&p.name).await.ok_or_else(|| {
            mcp::ErrorData::invalid_params(
                format!("library `{}` is not registered", p.name),
                None,
            )
        })?;
        let _guard = lock.write().await;
        reimport_library_impl(&p.name, &self.lint_engine).map_err(import_error_to_mcp_error)?;
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Drop a library and all its functions from the MCP-managed canonical repo. Does not touch the agent's local mirror."
    )]
    async fn unregister_library(
        &self,
        mcp::Parameters(p): mcp::Parameters<UnregisterLibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Re-evaluate a cached stateless closure by rerun_id with new args.",
        output_schema = mcp::schema_for_type::<RerunEnvelope>()
    )]
    async fn rerun(
        &self,
        mcp::Parameters(p): mcp::Parameters<RerunParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
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
            args: p.args.clone(),
            body: cached.body,
            timeout_ms: p.timeout_ms,
        };
        let source = build_run_source(&reconstructed);
        let payload_bytes = json::to_vec(&reconstructed).map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("serialize reconstructed RunParams for nonce: {e}"),
                None,
            )
        })?;
        let args_json = serde_json::Value::Object(p.args);
        let outcome = dispatch_pooled(
            &self.runs_pool,
            &self.nonce_gen,
            &self.in_flight,
            CacheKind::Runs,
            &payload_bytes,
            source,
            "rerun",
            args_json,
            InFlightKind::Rerun { rerun_id: p.rerun_id.clone() },
            p.timeout_ms,
        )
        .await?;
        let result_obj = outcome.result.as_object().cloned().unwrap_or_default();
        envelope_to_structured(&RerunEnvelope {
            result: result_obj,
            nonce: outcome.nonce.to_string(),
        })
    }

    #[mcp::tool(
        description = "Snapshot every in-flight tool call on the host. Returns an array of entries with the shape {nonce, tool, started_at, args, ...tool-specific}: for `run`/`interact` no extras; for `rerun` includes `rerun_id`; for `call` includes a flat `path` string `library:module/path:name` (with `library::name` when module_path is empty). Pair with `kill(nonce)` to cancel a specific call.",
        output_schema = mcp::schema_for_type::<ProcessesEnvelope>()
    )]
    async fn processes(
        &self,
        mcp::Parameters(_p): mcp::Parameters<ProcessesParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let map = self.in_flight.lock().await;
        let entries: Vec<ProcessEntry> = map
            .iter()
            .map(|(nonce_str, entry)| {
                let args_obj = entry
                    .args
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                let (rerun_id, path) = match &entry.kind {
                    InFlightKind::Run | InFlightKind::Interact => (None, None),
                    InFlightKind::Rerun { rerun_id } => (Some(rerun_id.clone()), None),
                    InFlightKind::Call { path } => (None, Some(path.clone())),
                };
                ProcessEntry {
                    nonce: nonce_str.clone(),
                    tool: entry.tool.to_string(),
                    started_at: entry.started_at,
                    args: args_obj,
                    rerun_id,
                    path,
                }
            })
            .collect();
        drop(map);
        envelope_to_structured(&ProcessesEnvelope { processes: entries })
    }

    #[mcp::tool(
        description = "Cancel an in-flight call by its nonce. SIGKILLs the worker holding the call; runs-pool workers are reaped and the next acquire spawns a fresh worker, interact respawn loses session state. Returns no payload; silently no-ops if the nonce is unknown or already completed (race-safe)."
    )]
    async fn kill(
        &self,
        mcp::Parameters(p): mcp::Parameters<KillParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let map = self.in_flight.lock().await;
        if let Some(entry) = map.get(&p.nonce) {
            let pid = entry.pid;
            drop(map);
            kill_worker_pid(pid);
        }
        Ok(mcp::CallToolResult::default())
    }

    #[mcp::tool(
        description = "Name, version, nu version, and nu plugins",
        output_schema = mcp::schema_for_type::<InfoEnvelope>()
    )]
    async fn info(
        &self,
        mcp::Parameters(_p): mcp::Parameters<InfoParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        envelope_to_structured(&InfoEnvelope {
            name: build_target().name().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            nu_version: env!("NU_VERSION").to_string(),
            plugins: list_registered_plugins(),
        })
    }
}

/// What: serializes any envelope struct into a `CallToolResult`
/// carrying only `structured_content` (no `content[]` text mirror).
/// Returns the rmcp shape Claude Code accepts directly via its
/// 2025-11-25 `outputSchema` validator.
///
/// Why: every C2 / C3 handler emits its typed envelope through this
/// seam so the structured-only choice (no content[] mirror) lives in
/// one place. If we ever need to flip to the spec-recommended
/// `structured + text mirror`, the swap is a one-line change to
/// `CallToolResult::structured`.
///
/// Where: called by every `#[mcp::tool]` handler in `NuSh` on the
/// success path. Errors still flow through `Err(ErrorData)`.
fn envelope_to_structured<T: ser::Serialize>(envelope: &T) -> Result<mcp::CallToolResult, mcp::ErrorData> {
    let value = json::to_value(envelope).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("envelope serialize: {e}"),
            None,
        )
    })?;
    let mut result = mcp::CallToolResult::default();
    result.structured_content = Some(value);
    Ok(result)
}

/// What: lint of a `RunParams` -- a single pass over the agent's
/// body. Returns the aggregated `LintViolation` vector with no source
/// tag (the body is the only context).
///
/// Why: keeping this as a thin wrapper around `lint_body` (rather
/// than inlining into the handlers) leaves a clear seam for any
/// future per-tool diff in lint coverage; today the wrapper is a
/// straight pass-through.
///
/// Where: called by `NuSh::run` and `NuSh::interact` before any
/// template synthesis. The result feeds `format_lint_violations` on
/// non-empty.
fn lint_run_params(engine: &ParseEngine, p: &RunParams) -> Vec<LintViolation> {
    lint_body(engine, &p.args_schema, &p.body, None)
}

/// What: the shape returned by `dispatch_pooled` / `dispatch_interact`.
/// Pairs the per-call `Nonce` with the decoded JSON `result` value.
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

/// What: dispatch one tool call against the stateless `runs_pool`.
/// Acquires a worker from the pool, registers an in-flight entry,
/// wraps the round-trip in `tk::timeout`, kills the worker on
/// timeout, returns the `DispatchOutcome` or an MCP-shaped error.
///
/// Why: replaces the prior single-worker `dispatch_to_worker` for
/// run/rerun/call. The pool gives concurrent execution; in-flight
/// tracking lets `kill(nonce)` and timeouts target a specific call's
/// worker; timeout wrap caps every call by the_user 2026-06-01
/// default of 120s when `timeout_ms` is omitted.
///
/// Where: called by `NuSh::run`, `NuSh::rerun`, `NuSh::call` (all
/// stateless surfaces).
async fn dispatch_pooled(
    pool: &Arc<Pool>,
    nonce_gen: &Arc<lib_empower::NonceGen>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    log_kind: CacheKind,
    payload_for_nonce: &[u8],
    source: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, mcp::ErrorData> {
    let nonce = nonce_gen.next(&payload_for_nonce);
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(log_kind, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("create_dir_all {}: {e}", log_dir.display()),
            None,
        )
    })?;
    let mut guard = pool.acquire().await.map_err(|e| {
        mcp::ErrorData::internal_error(format!("pool acquire: {e}"), None)
    })?;
    let pid = guard.pid();
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        tool_name,
        args_json,
        pid,
        kind,
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let send_fut = guard.send_request(log_dir, source);
    let timed = tk::timeout(
        tk::TkDuration::from_millis(effective_timeout),
        send_fut,
    )
    .await;
    let response = match timed {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            guard.drop_handle();
            return Err(mcp::ErrorData::internal_error(e.to_string(), None));
        }
        Err(_) => {
            kill_worker_pid(pid);
            guard.drop_handle();
            return Err(timeout_error(effective_timeout));
        }
    };
    drop(guard);
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

/// What: dispatch one `interact()` call against the single
/// stateful worker. Acquires the mutex, lazily respawns the worker if
/// None (after a prior kill / timeout / death cleared it), registers
/// in-flight, wraps in `tk::timeout`, kills + clears on timeout or
/// io error so the NEXT call lazy-respawns.
///
/// Why: the stateful worker is a singleton -- it can't pool because
/// session state is per-worker. Lazy-respawn lets kill/timeout clear
/// the handle without leaving callers stuck on a dead worker; the
/// next interact() spawns a fresh one (losing session state, as
/// the_user 2026-06-01 confirmed).
///
/// Where: called only by `NuSh::interact`.
async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
    nonce_gen: &Arc<lib_empower::NonceGen>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    payload_for_nonce: &[u8],
    source: String,
    args_json: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, mcp::ErrorData> {
    let nonce = nonce_gen.next(&payload_for_nonce);
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(CacheKind::Interacts, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| {
        mcp::ErrorData::internal_error(
            format!("create_dir_all {}: {e}", log_dir.display()),
            None,
        )
    })?;
    let mut worker_lock = interact.lock().await;
    if worker_lock.is_none() {
        let spawned = WorkerHandle::spawn(Mode::Stateful).await.map_err(|e| {
            mcp::ErrorData::internal_error(
                format!("interact respawn: {e}"),
                None,
            )
        })?;
        *worker_lock = Some(spawned);
    }
    let pid = worker_lock
        .as_ref()
        .expect("interact handle present after spawn")
        .pid();
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        "interact",
        args_json,
        pid,
        InFlightKind::Interact,
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let handle = worker_lock
        .as_mut()
        .expect("interact handle present after spawn");
    let send_fut = handle.send_request(log_dir, source);
    let timed = tk::timeout(
        tk::TkDuration::from_millis(effective_timeout),
        send_fut,
    )
    .await;
    let response = match timed {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            *worker_lock = None;
            return Err(mcp::ErrorData::internal_error(e.to_string(), None));
        }
        Err(_) => {
            kill_worker_pid(pid);
            *worker_lock = None;
            return Err(timeout_error(effective_timeout));
        }
    };
    drop(worker_lock);
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

async fn register_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    nonce_str: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    pid: u32,
    kind: InFlightKind,
) {
    let mut map = in_flight.lock().await;
    map.insert(
        nonce_str,
        InFlightEntry {
            tool: tool_name,
            started_at: now_millis(),
            args: args_json,
            pid,
            kind,
        },
    );
}

struct InFlightCleanup {
    map: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    key: String,
}

impl Drop for InFlightCleanup {
    fn drop(&mut self) {
        let map = self.map.clone();
        let key = std::mem::take(&mut self.key);
        tk::spawn(async move {
            let mut g = map.lock().await;
            g.remove(&key);
        });
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn timeout_error(timeout_ms: u64) -> mcp::ErrorData {
    mcp::ErrorData {
        code: mcp::ErrorCode(TIMEOUT_ERROR_CODE),
        message: format!(
            "timeout: closure exceeded {timeout_ms}ms; worker was killed",
        )
        .into(),
        data: None,
    }
}

/// What: writes the closure cache file at `closures/<rerun_id>.json`
/// after a successful `run()`. Creates the parent dir if needed,
/// serializes the closure metadata (`args_schema`, `result_schema`,
/// `body`) into `ClosureCacheBody`, and writes the JSON bytes.
/// Idempotent: the same rerun_id always produces the same bytes.
///
/// Why: rerun() needs a deterministic place to look up the cached
/// closure by its content-derived id. The unconditional overwrite
/// touches mtime even on identical content, which sets up future
/// LRU-style pruning. Returns `io::Result<()>` (slice 6.0): the caller
/// treats failures as non-fatal -- the agent still gets the successful
/// eval result, with the `rerun_id` field omitted from the envelope so
/// the agent knows replay is unavailable for this call.
///
/// Where: called by `NuSh::run` after `dispatch_to_worker` succeeds
/// and `RerunHash::of` produces the rerun_id. The matching read
/// happens in `NuSh::rerun` via `fs::read` + `json::from_slice`.
fn write_closure_cache(
    rerun_id: &str,
    p: &RunParams,
) -> io::Result<()> {
    let path = closure_cache_file(rerun_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = ClosureCacheBody {
        args_schema: p.args_schema.clone(),
        result_schema: p.result_schema.clone(),
        body: p.body.clone(),
    };
    let bytes = json::to_vec(&body).map_err(|e| {
        io::Error::other(format!("serialize ClosureCacheBody: {e}"))
    })?;
    fs::write(&path, &bytes)?;
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
            build_target().name(),
            env!("CARGO_PKG_VERSION"),
        )
        .with_title(match build_target() {
            BuildTarget::Main => "nushell",
            BuildTarget::Test => "nushell (test)",
        });
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
