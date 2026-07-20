use crate::*;

/// Parameters for `run()` / `interact()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RunParams {
    /// The strictly typed Nu `record` schema for `$args`, as a JSON object mapping each field name to its type (`{}` for no arguments).
    pub args_schema: mcp::JsonObject,
    /// The strictly typed Nu `record` schema for the return value, as a JSON object mapping each field name to its type (`{}` for no return value).
    pub result_schema: mcp::JsonObject,
    /// JSON object representation of the strictly typed Nu `record` schema for `$args` as passed to the source-code body.
    pub args: mcp::JsonObject,
    /// The nushell source-code body; its final value must match result_schema.
    pub body: String,
    /// Optional per-call timeout in milliseconds; defaults to 120000 (2 minutes). The usage is cancelled if it exceeds this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// The cached run body -- the converted positional types plus the source body,
/// enough to re-synthesize the run source for `rerun(nonce, args)`. Persisted as
/// NUON at `runs/<nonce>/body.nuon` (the house format for persisted nu data),
/// co-located with the call's stdout/stderr so a single prune of `runs/<nonce>/`
/// reclaims logs and body together.
pub(crate) struct CachedRunBody {
    pub(crate) args_type: String,
    pub(crate) result_type: String,
    pub(crate) body: String,
}

impl CachedRunBody {
    pub(crate) fn to_nuon(&self) -> Result<String, String> {
        let span = nu::Span::unknown();
        let mut record = nu::Record::new();
        record.insert("args_type", nu::Value::string(self.args_type.clone(), span));
        record.insert("result_type", nu::Value::string(self.result_type.clone(), span));
        record.insert("body", nu::Value::string(self.body.clone(), span));
        nu::to_nuon(
            &nu::EngineState::new(),
            &nu::Value::record(record, span),
            nu::ToNuonConfig::default(),
        )
        .map_err(|e| e.to_string())
    }

    pub(crate) fn from_nuon(text: &str) -> Result<Self, String> {
        let value = nu::from_nuon(text, None).map_err(|e| e.to_string())?;
        let record = value.as_record().map_err(|e| e.to_string())?;
        let field = |name: &str| -> Result<String, String> {
            match record.get(name) {
                Some(nu::Value::String { val, .. }) => Ok(val.clone()),
                Some(_) => Err(format!("cached body field `{name}` is not a string")),
                None => Err(format!("cached body missing `{name}`")),
            }
        };
        Ok(Self {
            args_type: field("args_type")?,
            result_type: field("result_type")?,
            body: field("body")?,
        })
    }
}

/// Persist the run body beside the call's logs (`runs/<nonce>/body.nuon`).
/// Non-fatal: a write failure is logged and the call proceeds -- the run still
/// returns its result + nonce; only a later `rerun(nonce)` would miss the body.
fn write_run_body(
    log_dir: &std::path::Path,
    body: &CachedRunBody,
) {
    let path = log_dir.join(BODY_FILE);
    match body.to_nuon() {
        Ok(nuon) => {
            if let Err(e) = fs::write(&path, nuon.as_bytes()) {
                eprintln!("nushell_mcp: run body cache write failed at {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("nushell_mcp: run body cache serialize failed: {e}"),
    }
}

pub struct NuSh {
    pub(crate) interact_engine: Arc<tk::AsyncMutex<Option<InteractEngine>>>,
    /// The in-process stateless engine base - each eval clones it onto a
    /// generously-stacked blocking thread (replaces the worker pool).
    pub(crate) base: Arc<nu::EngineState>,
    /// Host-owned environment-wide jobs table injected into every eval clone
    /// (P0.8) - a body's `job spawn` persists + is visible/killable across evals.
    pub(crate) env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
    /// Bounds concurrent in-process evals (the former worker-pool cap).
    pub(crate) eval_semaphore: Arc<tk::Semaphore>,
    pub(crate) nonce_gen: Arc<NonceGen>,
    pub(crate) library_locks: Arc<LibraryLocks>,
    pub(crate) lint_engine: Arc<ParseEngine>,
    pub(crate) in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    pub(crate) tool_router: mcp::ToolRouter<NuSh>,
}

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 120_000;


pub(crate) struct InFlightEntry {
    pub tool: &'static str,
    pub started_at: u64,
    pub args: serde_json::Value,
    pub kind: InFlightKind,
}

pub(crate) enum InFlightKind {
    Run,
    Interact,
    Rerun { source_nonce: String },
    Call { path: String },
}

impl NuSh {
    pub(crate) fn new(
        nonce_gen: Arc<NonceGen>,
        library_locks: Arc<LibraryLocks>,
        lint_engine: Arc<ParseEngine>,
    ) -> Self {
        // Install nushell's TLS crypto provider once for the in-process engine
        // (the http family reads nushell's own OnceLock; formerly per-worker).
        nu::CRYPTO_PROVIDER.default();
        let base = Arc::new(build_base(Mode::Stateless));
        let env_jobs = Arc::new(std::sync::Mutex::new(nu::Jobs::default()));
        let eval_semaphore = Arc::new(tk::Semaphore::new(worker_pool_cap()));
        Self {
            interact_engine: Arc::new(tk::AsyncMutex::new(None)),
            base,
            env_jobs,
            eval_semaphore,
            nonce_gen,
            library_locks,
            lint_engine,
            in_flight: Arc::new(tk::AsyncMutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
        }
    }
}



pub(crate) fn envelope_to_structured<T: ser::Serialize>(
    envelope: &T,
) -> Result<mcp::CallToolResult, mcp::ErrorData> {
    let value = json::to_value(envelope)
        .map_err(|e| mcp::ErrorData::internal_error(format!("envelope serialize: {e}"), None))?;
    let mut result = mcp::CallToolResult::default();
    result.structured_content = Some(value);
    Ok(result)
}

pub(crate) fn convert_schemas(
    args_schema: &mcp::JsonObject,
    result_schema: &mcp::JsonObject,
) -> Result<(String, String), String> {
    let args_type = args_schema_to_nu(args_schema)?;
    let result_type = result_schema_to_nu(result_schema)?;
    Ok((args_type, result_type))
}

pub(crate) fn lint_run_params(
    engine: &ParseEngine,
    args_type: &str,
    body: &str,
) -> Vec<Diagnostic> {
    lint_body(engine, args_type, body)
}

pub(crate) struct DispatchOutcome {
    pub(crate) nonce: Nonce,
    pub(crate) result: json::Value,
}

pub(crate) struct DispatchError {
    pub(crate) error: Error,
    pub(crate) nonce: Option<Nonce>,
}

pub(crate) async fn dispatch_pooled(
    base: &Arc<nu::EngineState>,
    env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>,
    eval_semaphore: &Arc<tk::Semaphore>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    log_kind: CacheKind,
    nonce: Nonce,
    source: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
    cache_body: Option<CachedRunBody>,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, DispatchError> {
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(log_kind, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: Error::Internal {
            phase: "dispatch_pooled::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
    if let Some(body) = &cache_body {
        write_run_body(&log_dir, body);
    }
    let permit = eval_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| DispatchError {
            error: Error::WorkerDispatch {
                reason: format!("eval semaphore: {e}"),
            },
            nonce: None,
        })?;
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        tool_name,
        args_json,
        kind,
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = eval_stateless(base.clone(), env_jobs.clone(), log_dir, source, permit);
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce, result }),
        Ok(Err(reason)) => Err(DispatchError {
            error: Error::WorkerReturnedError { reason },
            nonce: Some(nonce),
        }),
        Err(_) => Err(DispatchError {
            // Phase 1 (host-unsafe intermediate): no cancellation yet - the eval
            // thread runs on (holding its permit) until it finishes on its own;
            // Phase 2 wires Signals + the teardown so a timeout actually reaps it.
            error: Error::WorkerTimeout {
                timeout_ms: effective_timeout,
            },
            nonce: Some(nonce),
        }),
    }
}

pub(crate) async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<InteractEngine>>>,
    env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    nonce: Nonce,
    source: String,
    args_json: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, DispatchError> {
    let nonce_str = nonce.to_string();
    let log_dir = cache_dir(CacheKind::Interacts, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: Error::Internal {
            phase: "dispatch_interact::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
    // Lazily spawn the persistent interact engine (a dedicated 64 MB thread owning
    // the stateful EngineState), then clone the cheap handle out so the eval does
    // not hold the guard - the engine thread serializes calls itself.
    let engine = {
        let mut guard = interact.lock().await;
        if guard.is_none() {
            *guard = Some(InteractEngine::spawn(env_jobs.clone()));
        }
        guard
            .as_ref()
            .expect("interact engine present after spawn")
            .clone()
    };
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        "interact",
        args_json,
        InFlightKind::Interact,
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = engine.eval(log_dir, source);
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce, result }),
        Ok(Err(reason)) => Err(DispatchError {
            error: Error::WorkerReturnedError { reason },
            nonce: Some(nonce),
        }),
        // Phase 1 (host-unsafe intermediate): the timeout returns but the interact
        // thread runs its eval to completion - no cancellation yet (Phase 2).
        Err(_) => Err(DispatchError {
            error: Error::WorkerTimeout {
                timeout_ms: effective_timeout,
            },
            nonce: Some(nonce),
        }),
    }
}

async fn register_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    nonce_str: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
) {
    let mut map = in_flight.lock().await;
    map.insert(
        nonce_str,
        InFlightEntry {
            tool: tool_name,
            started_at: now_millis(),
            args: args_json,
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


