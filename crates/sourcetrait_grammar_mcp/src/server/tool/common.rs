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
                eprintln!("grammar: run body cache write failed at {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("grammar: run body cache serialize failed: {e}"),
    }
}

pub struct NuSh {
    pub(crate) interact_engine: Arc<tk::AsyncMutex<Option<InteractEngine>>>,
    /// The stateless eval executor: a swappable base + a pre-cloned ready buffer,
    /// concurrency-bounded; hands each eval a ready clone (server/executor.rs).
    pub(crate) executor: Arc<Executor>,
    /// Host-owned environment-wide jobs table shared into every eval (P0.8) - a
    /// body's `job spawn` persists + is visible/killable across evals + the
    /// interact lane.
    pub(crate) env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
    pub(crate) nonce_gen: Arc<NonceGen>,
    /// This host process's identity, minted once at construction. Namespaces the
    /// per-process emergency log and is reported by `info()`.
    pub(crate) mcp_nom: McpNom,
    pub(crate) library_locks: Arc<LibraryLocks>,
    pub(crate) lint_engine: Arc<ParseEngine>,
    /// The resource registry, keyed by nonce string: each eval's cancel handle
    /// plus the self-matching {tool, started_at, args, kind}. processes()
    /// snapshots it; kill(nonce) triggers the cancel handle.
    pub(crate) in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    /// The hang-detection registry (server/watchdog.rs): cancelled engine threads
    /// that may still be alive, populated on timeout / kill. The watchdog prunes
    /// finished entries and confirms hangs past grace.
    pub(crate) hung_watch: HungRegistry,
    pub(crate) tool_router: mcp::ToolRouter<NuSh>,
}

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Pre-clone depth N for the stateless executor's ready buffer (keep 1 ready).
pub(crate) const READY_POOL_TARGET: usize = 1;


pub(crate) struct InFlightEntry {
    pub tool: &'static str,
    pub started_at: u64,
    pub args: serde_json::Value,
    pub kind: InFlightKind,
    /// This eval's interrupt flag - the `Signals` handle the eval thread polls.
    /// kill(nonce) / a timeout flips it to cancel the eval cooperatively.
    pub cancel: Arc<AtomicBool>,
    /// This eval's external-child tracker (Arc-shared pids). kill / timeout reads
    /// collect_pids() + tree-kills the process tree (server/teardown.rs).
    pub tracker: nu::ThreadJob,
    /// Liveness: the eval thread flips this true on exit via a Drop guard (a
    /// caught panic still exits, so it flips; only a thread stuck where it never
    /// returns leaves it false). The watchdog reads it to confirm / prune hangs;
    /// kill(nonce) copies it into a HungWatch (server/watchdog.rs).
    pub finished: Arc<AtomicBool>,
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
        let env_jobs = Arc::new(std::sync::Mutex::new(nu::Jobs::default()));
        let executor = Arc::new(Executor::new(env_jobs.clone(), READY_POOL_TARGET));
        let mcp_nom = McpNom::mint(&nonce_gen);
        Self {
            interact_engine: Arc::new(tk::AsyncMutex::new(None)),
            executor,
            env_jobs,
            nonce_gen,
            mcp_nom,
            library_locks,
            lint_engine,
            in_flight: Arc::new(tk::AsyncMutex::new(HashMap::new())),
            hung_watch: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
    executor: &Arc<Executor>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    hung_watch: &HungRegistry,
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
    // Pick up a plugin add/rm (or an external registry edit) before taking a
    // clone, so this eval runs against current plugin decls (server/executor.rs).
    executor.refresh_base_if_stale();
    let permit = executor
        .semaphore()
        .acquire_owned()
        .await
        .map_err(|e| DispatchError {
            error: Error::ThreadDispatch {
                reason: format!("eval semaphore: {e}"),
            },
            nonce: None,
        })?;
    let mut engine = executor.take_clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let started_at = now_millis();
    let finished = Arc::new(AtomicBool::new(false));
    // Track this eval's external children so a cancel/timeout reaps the whole
    // process tree (server/teardown.rs): nushell registers each external's pid
    // into the tracker once it is the engine's background_thread_job.
    let tracker = make_tracker(cancel.clone());
    engine.current_job.background_thread_job = Some(tracker.clone());
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        tool_name,
        args_json,
        kind,
        started_at,
        cancel.clone(),
        tracker.clone(),
        finished.clone(),
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = eval_stateless(engine, cancel.clone(), log_dir, source, permit, finished.clone());
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce, result }),
        Ok(Err(reason)) => Err(DispatchError {
            error: Error::ThreadReturnedError { reason },
            nonce: Some(nonce),
        }),
        Err(_) => {
            // Trigger the eval's Signals so the abandoned thread bails at nushell's
            // next check point + releases its permit, and reap any external process
            // tree it spawned (a pure-Rust hung eval cannot be reached - the residual).
            cancel.store(true, Ordering::SeqCst);
            tree_kill(&tracker.collect_pids());
            // Record the cancelled eval for the watchdog: an engine thread still
            // alive past grace is a confirmed hang (server/watchdog.rs).
            register_hung(
                hung_watch,
                HungWatch {
                    nonce: nonce.to_string(),
                    tool: tool_name,
                    lane: Lane::Stateless,
                    started_at,
                    cancelled_at: now_millis(),
                    finished: finished.clone(),
                },
            );
            Err(DispatchError {
                error: Error::ThreadTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce),
            })
        }
    }
}

pub(crate) async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<InteractEngine>>>,
    env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    hung_watch: &HungRegistry,
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
    let cancel = Arc::new(AtomicBool::new(false));
    let tracker = make_tracker(cancel.clone());
    let started_at = now_millis();
    let finished = Arc::new(AtomicBool::new(false));
    register_in_flight(
        in_flight,
        nonce_str.clone(),
        "interact",
        args_json,
        InFlightKind::Interact,
        started_at,
        cancel.clone(),
        tracker.clone(),
        finished.clone(),
    )
    .await;
    let _flight_cleanup = InFlightCleanup {
        map: in_flight.clone(),
        key: nonce_str,
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = engine.eval(log_dir, source, cancel.clone(), tracker.clone(), finished.clone());
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce, result }),
        Ok(Err(reason)) => Err(DispatchError {
            error: Error::ThreadReturnedError { reason },
            nonce: Some(nonce),
        }),
        Err(_) => {
            // Trigger the current interact eval's Signals + reap its external tree;
            // a non-hung eval bails and the serial lane frees for the next call (a
            // hung interact lane is Phase 4's respawn).
            cancel.store(true, Ordering::SeqCst);
            tree_kill(&tracker.collect_pids());
            // Record the cancelled interact eval for the watchdog (followup #41): a
            // thread still alive past grace is the interact-lane hang.
            register_hung(
                hung_watch,
                HungWatch {
                    nonce: nonce.to_string(),
                    tool: "interact",
                    lane: Lane::Interact,
                    started_at,
                    cancelled_at: now_millis(),
                    finished: finished.clone(),
                },
            );
            Err(DispatchError {
                error: Error::ThreadTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce),
            })
        }
    }
}

async fn register_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    nonce_str: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
    started_at: u64,
    cancel: Arc<AtomicBool>,
    tracker: nu::ThreadJob,
    finished: Arc<AtomicBool>,
) {
    let mut map = in_flight.lock().await;
    map.insert(
        nonce_str,
        InFlightEntry {
            tool: tool_name,
            started_at,
            args: args_json,
            kind,
            cancel,
            tracker,
            finished,
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

pub(crate) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Clean-shutdown teardown: cancel every still-in-flight eval and reap its
/// external process tree + the plugin subprocesses. Called after the MCP service
/// stops (the client closed stdin), so a disconnect mid-eval never leaks a
/// process tree.
pub(crate) async fn teardown_all_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
) {
    let pids: Vec<u32> = {
        let map = in_flight.lock().await;
        let mut all = Vec::new();
        for entry in map.values() {
            entry.cancel.store(true, Ordering::SeqCst);
            all.extend(entry.tracker.collect_pids());
        }
        all
    };
    tree_kill(&pids);
    kill_plugin_subprocesses();
}


