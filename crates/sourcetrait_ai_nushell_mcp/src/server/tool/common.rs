use crate::*;

/// Parameters for `run()` / `interact()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RunParams {
    /// The strictly typed Nu `record` schema for `$args`, as a JSON object mapping each field name to its type (`{}` for no arguments).
    pub args_schema: mcp::JsonObject,
    /// The strictly typed Nu `record` schema for the return value, as a JSON object mapping each field name to its type (`{}` for no return value).
    pub result_schema: mcp::JsonObject,
    /// JSON object representation of the strictly typed Nu `record` schema for `$args` as passed to the source-code body.
    // `mcp::JsonObject` (not `serde_json::Value`): schemars renders Value as the JSON Schema `true` keyword, which the MCP client rejects; a Map renders as `{"type": "object"}`. args must be an object anyway -- it deserializes into a nu record.
    pub args: mcp::JsonObject,
    pub body: String,
    /// Optional per-call timeout in milliseconds; defaults to 120000 (2 minutes). The usage is cancelled if it exceeds this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// What: on-disk JSON shape of `closures/<rerun_id>.json`. Mirrors
/// the relevant subset of `RunParams` that uniquely identifies the
/// closure: schemas + body, no per-call args, no nonce.
///
/// Why: rerun() needs enough to reconstruct a `RunParams` and
/// dispatch through the stateless worker; the schemas are the
/// typecheck inputs, the body is the executable surface.
///
/// Where: serialized by `write_closure_cache` (in `tool::run`) after a
/// successful `NuSh::run`; deserialized by `NuSh::rerun` via
/// `json::from_slice` to reconstruct a `RunParams` for replay.
#[derive(Debug, ser::Deserialize, ser::Serialize)]
pub(crate) struct ClosureCacheBody {
    pub(crate) args_type: String,
    pub(crate) result_type: String,
    pub(crate) body: String,
}

/// What: the rmcp server-side state. Owns Arc-wrapped handles to the
/// two worker subprocesses (stateless + stateful), the NonceGen for
/// per-call ids, the per-library lock registry for slice-3
/// concurrency, and rmcp's `ToolRouter` (assembled from the per-tool
/// router functions across the `server::tool::*` modules).
///
/// Why: this is THE singleton the rmcp library serves. Every Arc
/// field is cheap to clone for concurrent tool calls; the workers
/// sit behind AsyncMutex because each worker is a single-request
/// channel; the library_locks sits behind its own internal
/// concurrency primitive. Fields are `pub(crate)` so the per-tool
/// handler modules reach them as `self.<field>`.
///
/// Where: constructed in `server::run::run_server` after substrate
/// init and worker spawn; passed to `service.serve(stdio())` which
/// runs the MCP protocol against the host's stdin/stdout. Every
/// `#[mcp::tool]` method across the `server::tool::*` modules is a
/// tool surface entry on this type.
pub struct NuSh {
    pub(crate) runs_pool: Arc<Pool>,
    pub(crate) interact_worker: Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
    pub(crate) nonce_gen: Arc<lib_empower::NonceGen>,
    pub(crate) library_locks: Arc<LibraryLocks>,
    pub(crate) lint_engine: Arc<ParseEngine>,
    pub(crate) in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    #[allow(dead_code)]
    pub(crate) tool_router: mcp::ToolRouter<NuSh>,
}

/// Default per-call timeout when `timeout_ms` is omitted (the_user
/// 2026-06-01: 120s catches hangs without imposing an upper cap).
pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 120_000;

// TIMEOUT_ERROR_CODE retired in 0.0.34 -- timeouts now emit a typed
// `Error::WorkerTimeout { timeout_ms }` envelope rather than a JSON-
// RPC error code. Agent branches on `structuredContent.error.kind ==
// "worker::timeout"` instead of `error.code == -32001`.

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
/// call), removed via `InFlightCleanup::drop` when dispatch returns.
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

impl NuSh {
    /// What: constructor for `NuSh`. Wraps the two `WorkerHandle`
    /// values in `Arc<tk::AsyncMutex<...>>` for shared serialized
    /// access, takes the already-Arc-wrapped nonce_gen + library_locks
    /// directly, and initializes the rmcp `tool_router` from the
    /// combined `Self::tool_router()` (assembled in `tool::handler`).
    ///
    /// Why: all the wrapping happens here so calling code in
    /// `run_server` can pass plain `WorkerHandle`s and Arc references
    /// without juggling layers. The combined tool_router has to be
    /// initialized AFTER all the inputs are owned, hence initialization
    /// at the bottom of the field block.
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
}

// outputSchema deviation note (the_user 2026-06-02): a `oneOf(Success,
// ErrorEnvelope)` wrapper was attempted to formally cover both branches
// of each tool's wire shape, but Claude Code's MCP client rejected the
// resulting schemas (root-level `oneOf` lacks `type: "object"`). The
// pragmatic outcome: per-tool `output_schema` declares the SUCCESS
// envelope shape only. Error responses at runtime emit
// `structuredContent.error.{kind, data, nonce?}` per the new error
// envelope; they don't validate against the declared schema but
// transit + render fine (Claude Code does not enforce validation on
// tool results at the structured-content layer). The
// `Error`/`ErrorEnvelope`/`Where` types are still typed Rust internals
// (built by handlers + serialized to JSON); only the schema-declaration
// side of the wire contract is success-only.

// Content::text omission deviation note (the_user 2026-06-02).
//
// MCP 2025-11-25 (server/tools.md) says verbatim:
//
//   "For backwards compatibility, a tool that returns structured
//    content SHOULD also return the serialized JSON in a TextContent
//    block."
//
// We ignore that SHOULD. Every nushell_mcp tool handler emits
// `structured_content: Some(<typed envelope>)` and leaves
// `content: vec![]`. No text mirror. We own any wire weirdness this
// causes for MCP clients that depend on the text mirror for display
// or for parsing -- such clients will not see our envelope contents.
//
// What we ship instead: our own opinionated typed envelopes
// (`RunEnvelope` / `CallEnvelope` / `InteractEnvelope` / `RerunEnvelope`
// / `InfoEnvelope` / `ProcessesEnvelope` / `ErrorEnvelope`) on
// `structured_content`. Agents read fields directly off the structured
// payload. The wire is uniform across success and error semantics; both
// emit success-shape via `envelope_to_structured` (success path) and
// `error_to_call_result` (error path, in `server/error.rs`).
//
// Domain vs protocol error policy:
//   - Domain errors (lint violations, library validation, worker
//     timeout, function-not-defined, closure cache miss, internal
//     phase failures, etc.) go through `error_to_call_result` ->
//     success-shape with `structuredContent.error` carrying the typed
//     `Error`. Agent branches on `structuredContent.error.kind`.
//   - Genuine MCP-layer failures (deserialize / serialize errors at
//     the rmcp boundary, unsupported request shapes) are reserved for
//     `Err(mcp::ErrorData)` JSON-RPC error responses. The current
//     handler set never hits this path; it remains available if a
//     future rmcp boundary case demands a true protocol error.
//
// Verified rendering in Claude Code's CLI (the_user observed
// 2026-06-02 via rmcp_explore Section E probes against the spec
// example shapes):
//   - success-shape + `structured_content` (no `Content::text`)
//     -> green bullet, body visible on click-expand (this is the
//     shape we ship on both success and error semantics)
//   - `is_error=true` + `structured_content` -> red bullet, no body
//   - `is_error=true` + `Content::text` -> red bullet, no body
//   - `is_error=true` + dual-emit (`Content::text` + `structured`)
//     -> red bullet, no body
//   - `Err(mcp::ErrorData)` JSON-RPC -32602 -> red bullet, no body
//
// Body visibility on the protocol-level error path is not achievable
// in current Claude Code's CLI. Success-shape carrying the typed error
// envelope is the only path that surfaces the body. This is why our
// error handler uses success-shape, not the spec-canonical
// `is_error=true + Content::text` pattern.
//
// Flip cost: adding the `Content::text` mirror per the SHOULD is a
// one-line addition in `envelope_to_structured` and
// `error_to_call_result` (serialize the envelope to JSON, push as a
// `Content::text` block alongside the existing structured payload).
// The structured payload stays. Flip if a future Claude Code version
// starts requiring the text mirror, or if portability to other MCP
// clients becomes a goal.

/// What: serializes the typed success envelope to `structured_content`
/// and emits an empty `content` vector. Deviates from MCP 2025-11-25
/// `server/tools.md` SHOULD (omits the `Content::text` mirror) -- see
/// the "Content::text omission deviation note" block above for the
/// verbatim spec quote and the policy.
///
/// Why: single seam so the structured-only choice lives in one place;
/// pairs with `error_to_call_result` in `server/error.rs` for the
/// error-side counterpart.
///
/// Where: called by every `#[mcp::tool]` handler across the
/// `server::tool::*` modules on the success path (run, interact,
/// rerun, call, info, processes, ...).
pub(crate) fn envelope_to_structured<T: ser::Serialize>(
    envelope: &T,
) -> Result<mcp::CallToolResult, mcp::ErrorData> {
    let value = json::to_value(envelope)
        .map_err(|e| mcp::ErrorData::internal_error(format!("envelope serialize: {e}"), None))?;
    let mut result = mcp::CallToolResult::default();
    result.structured_content = Some(value);
    Ok(result)
}

/// What: converts the structured `args_schema` + `result_schema`
/// JSON objects to the `(args_type, result_type)` nu positional-type
/// strings via `args_schema_to_nu` / `result_schema_to_nu`.
///
/// Why: run + interact need the converted
/// positional types BEFORE lint + template synthesis; a single helper
/// keeps the conversion seam in one place and short-circuits to
/// `Error::SchemaInvalid` uniformly on a malformed schema.
///
/// Where: called first by `NuSh::run` and `NuSh::interact`.
pub(crate) fn convert_schemas(
    args_schema: &mcp::JsonObject,
    result_schema: &mcp::JsonObject,
) -> Result<(String, String), String> {
    let args_type = args_schema_to_nu(args_schema)?;
    let result_type = result_schema_to_nu(result_schema)?;
    Ok((args_type, result_type))
}

/// What: lint of a `RunParams` body -- a single pass over the agent's
/// body returning the aggregated `LintViolation` vector with no source
/// tag (the body is the only context).
///
/// Why: keeping this as a thin wrapper around `lint_body` (rather
/// than inlining into the handlers) leaves a clear seam for any
/// future per-tool diff in lint coverage; today the wrapper is a
/// straight pass-through.
///
/// Where: called by `NuSh::run` and `NuSh::interact` before any
/// template synthesis.
pub(crate) fn lint_run_params(
    engine: &ParseEngine,
    args_type: &str,
    body: &str,
) -> Vec<LintViolation> {
    lint_body(engine, args_type, body, None)
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
/// Where: returned by `dispatch_pooled` / `dispatch_interact` to
/// each `#[mcp::tool]` handler that wraps it into a typed envelope.
pub(crate) struct DispatchOutcome {
    pub(crate) nonce: lib_empower::Nonce,
    pub(crate) result: json::Value,
}

/// What: error-side return value from `dispatch_pooled` /
/// `dispatch_interact`. Pairs the typed `Error` with an optional
/// `Nonce` -- present when a worker-side log dir at
/// `$XDG_CACHE_HOME/sourcetrait/nushell_mcp/<x>/<nonce>/` was created (the agent
/// can fetch stdout/stderr by that nonce).
///
/// Why: the dispatch helpers may fail BEFORE or AFTER allocating a
/// per-call nonce. Returning both `error` and optional `nonce`
/// (rather than embedding the nonce into a specific Error variant)
/// keeps the typed Error data shape uniform per kind while
/// preserving the agent's path back to the cached log dir.
///
/// Where: produced internally by the dispatch helpers; the handler
/// pipes through `error_to_call_result(error, nonce)` to convert
/// into the wire envelope.
pub(crate) struct DispatchError {
    pub(crate) error: Error,
    pub(crate) nonce: Option<lib_empower::Nonce>,
}

/// What: dispatch one tool call against the stateless `runs_pool`.
/// Acquires a worker from the pool, registers an in-flight entry,
/// wraps the round-trip in `tk::timeout`, kills the worker on
/// timeout, returns the `DispatchOutcome` or a typed `DispatchError`
/// (carrying both the `Error` and the per-call `Nonce` when present
/// so the handler can attach it to the wire envelope).
///
/// Why: replaces the prior `Err(mcp::ErrorData)` shape so each
/// error path emits a typed `Error` variant -- the handler then
/// routes through `error_to_call_result` for visible CLI rendering.
/// The pool gives concurrent execution; in-flight tracking lets
/// `kill(nonce)` and timeouts target a specific call's worker;
/// timeout wrap caps every call.
///
/// Where: called by `NuSh::run`, `NuSh::rerun`, `NuSh::call` (all
/// stateless surfaces).
pub(crate) async fn dispatch_pooled(
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
) -> Result<DispatchOutcome, DispatchError> {
    let nonce = nonce_gen.next(&payload_for_nonce);
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(log_kind, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: Error::Internal {
            phase: "dispatch_pooled::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
    let mut guard = pool.acquire().await.map_err(|e| DispatchError {
        error: Error::WorkerDispatch {
            reason: format!("pool acquire: {e}"),
        },
        nonce: None,
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
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), send_fut).await;
    let response = match timed {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            guard.drop_handle();
            return Err(DispatchError {
                error: Error::WorkerDispatch {
                    reason: e.to_string(),
                },
                nonce: Some(nonce),
            });
        }
        Err(_) => {
            kill_worker_pid(pid);
            guard.drop_handle();
            return Err(DispatchError {
                error: Error::WorkerTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce),
            });
        }
    };
    drop(guard);
    if !response.ok {
        return Err(DispatchError {
            error: Error::WorkerReturnedError {
                reason: response
                    .error
                    .unwrap_or_else(|| "worker returned ok=false with no error".to_string()),
            },
            nonce: Some(nonce),
        });
    }
    let result: json::Value = msgpack::from_slice(&response.value).unwrap_or(json::Value::Null);
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
pub(crate) async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
    nonce_gen: &Arc<lib_empower::NonceGen>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    payload_for_nonce: &[u8],
    source: String,
    args_json: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, DispatchError> {
    let nonce = nonce_gen.next(&payload_for_nonce);
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(CacheKind::Interacts, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: Error::Internal {
            phase: "dispatch_interact::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
    let mut worker_lock = interact.lock().await;
    if worker_lock.is_none() {
        let spawned = WorkerHandle::spawn(Mode::Stateful)
            .await
            .map_err(|e| DispatchError {
                error: Error::WorkerDispatch {
                    reason: format!("interact respawn: {e}"),
                },
                nonce: None,
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
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), send_fut).await;
    let response = match timed {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            *worker_lock = None;
            return Err(DispatchError {
                error: Error::WorkerDispatch {
                    reason: e.to_string(),
                },
                nonce: Some(nonce),
            });
        }
        Err(_) => {
            kill_worker_pid(pid);
            *worker_lock = None;
            return Err(DispatchError {
                error: Error::WorkerTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce),
            });
        }
    };
    drop(worker_lock);
    if !response.ok {
        return Err(DispatchError {
            error: Error::WorkerReturnedError {
                reason: response
                    .error
                    .unwrap_or_else(|| "worker returned ok=false with no error".to_string()),
            },
            nonce: Some(nonce),
        });
    }
    let result: json::Value = msgpack::from_slice(&response.value).unwrap_or(json::Value::Null);
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

// (timeout_error and TIMEOUT_ERROR_CODE retired; timeouts now emit
// Error::WorkerTimeout { timeout_ms } via DispatchError. See the
// timeout branches in `dispatch_pooled` / `dispatch_interact`.)

// (import_error_to_mcp_error / format_violations / format_validation_result
// retired -- errors are now typed `Error` values routed through
// `error_to_call_result` rather than rendered as text JSON-RPC errors.
// The structural `Violation` list and `LintViolation` list now live as
// typed data inside `Error::LibraryViolations` and `Error::LintViolations`.)
