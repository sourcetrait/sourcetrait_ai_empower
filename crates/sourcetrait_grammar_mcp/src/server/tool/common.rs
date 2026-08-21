use crate::*;

/// Parameters for `run()` / `interact()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RunParams {
    /// The args schema: field name to type; `{}` for no arguments.
    pub args_schema: mcp::JsonObject,
    /// The result schema: field name to type; `{}` for no return value.
    pub result_schema: mcp::JsonObject,
    /// JSON object of argument values passed to the body as `$args`.
    pub args: mcp::JsonObject,
    /// The nushell source-code body; its final value must match result_schema.
    pub body: String,
    /// Per-call timeout in milliseconds; defaults to 120000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// The cached run body: the converted types plus the source body.
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

/// Persist the run body beside the call's logs.
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
    /// The stateless eval executor; hands each eval a ready clone.
    pub(crate) executor: Arc<Executor>,
    /// Host-owned environment-wide jobs table shared into every eval.
    pub(crate) env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
    pub(crate) nonce_gen: Arc<datum::NonceGenerator>,
    /// This host process's identity, minted once at construction.
    pub(crate) mcp_nom: datum::NomPair,
    pub(crate) rig_locks: Arc<RigLocks>,
    /// The lint and rig-validator engine, taken through `current()`.
    pub(crate) lint_engine: Arc<LintEngine>,
    /// The resource registry, keyed by nonce string.
    pub(crate) in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    /// The hang-detection registry the watchdog scans.
    pub(crate) hung_watch: HungRegistry,
    /// The host's single packet channel; present but CLOSED at startup.
    pub(crate) channel: Arc<ChannelHandle>,
    /// Serializes `channel_open`'s decide-then-start across its await.
    pub(crate) channel_open_lock: Arc<tk::AsyncMutex<()>>,
    /// Which purview ids this host has in view; SESSION-resident.
    pub(crate) current_purview: Arc<CurrentPurview>,
    /// Open mTLS links to remote grammar hosts - a clone of the process-global
    /// registry, keyed by alias (initiator) or peer datum::Nom (accepted).
    pub(crate) remote_links: Arc<std::sync::Mutex<HashMap<String, RemoteLinkEntry>>>,
    pub(crate) tool_router: mcp::ToolRouter<NuSh>,
}

/// One open remote link plus the peer address, for the registry and listing.
pub(crate) struct RemoteLinkEntry {
    pub handle: RemoteLinkHandle,
    pub addr: std::net::SocketAddr,
}

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Pre-clone depth N for the stateless executor's ready buffer.
pub(crate) const READY_POOL_TARGET: usize = 1;

pub(crate) struct InFlightEntry {
    pub tool: &'static str,
    pub started_at: u64,
    pub args: serde_json::Value,
    pub kind: InFlightKind,
    /// This eval's interrupt flag - the `Signals` handle its thread polls.
    pub cancel: Arc<AtomicBool>,
    /// This eval's external-child tracker, holding Arc-shared pids.
    pub tracker: nu::ThreadJob,
    /// Liveness: the eval thread flips this on exit via a Drop guard.
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
        nonce_gen: Arc<datum::NonceGenerator>,
        rig_locks: Arc<RigLocks>,
        lint_engine: Arc<LintEngine>,
    ) -> Self {
        nu::CRYPTO_PROVIDER.default();
        let env_jobs = Arc::new(std::sync::Mutex::new(nu::Jobs::default()));
        let executor = Arc::new(Executor::new(env_jobs.clone(), READY_POOL_TARGET));
        let mcp_nom = datum::Nom::generate(&nonce_gen).into_pair();
        Self {
            interact_engine: Arc::new(tk::AsyncMutex::new(None)),
            executor,
            env_jobs,
            nonce_gen,
            mcp_nom,
            rig_locks,
            lint_engine,
            in_flight: Arc::new(tk::AsyncMutex::new(HashMap::new())),
            hung_watch: Arc::new(std::sync::Mutex::new(HashMap::new())),
            channel: channel_handle(),
            channel_open_lock: Arc::new(tk::AsyncMutex::new(())),
            current_purview: Arc::new(CurrentPurview::new()),
            remote_links: remote_links(),
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
    pub(crate) nonce: datum::Nonce,
    pub(crate) result: json::Value,
}

pub(crate) struct DispatchError {
    pub(crate) error: GrammarMcpError,
    pub(crate) nonce: Option<datum::Nonce>,
}

pub(crate) async fn dispatch_pooled(
    executor: &Arc<Executor>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    hung_watch: &HungRegistry,
    log_kind: CacheKind,
    nonce: datum::NoncePair,
    source: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
    cache_body: Option<CachedRunBody>,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, DispatchError> {
    let log_dir = cache_dir(log_kind, &nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: GrammarMcpError::Internal {
            phase: "dispatch_pooled::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
    if let Some(body) = &cache_body {
        write_run_body(&log_dir, body);
    }
    executor.refresh_base_if_stale();
    let permit = executor
        .semaphore()
        .acquire_owned()
        .await
        .map_err(|e| DispatchError {
            error: GrammarMcpError::ThreadDispatch {
                reason: format!("eval semaphore: {e}"),
            },
            nonce: None,
        })?;
    let mut engine = executor.take_clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let started_at = now_millis();
    let finished = Arc::new(AtomicBool::new(false));
    let tracker = make_tracker(cancel.clone());
    engine.current_job.background_thread_job = Some(tracker.clone());
    register_in_flight(
        in_flight,
        &nonce,
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
        key: nonce.as_str().to_string(),
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = eval_stateless(engine, cancel.clone(), log_dir, source, permit, finished.clone());
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce: nonce.to_nonce(), result }),
        Ok(Err(failure)) => Err(DispatchError {
            error: failure.into_error(),
            nonce: Some(nonce.to_nonce()),
        }),
        Err(_) => {
            cancel.store(true, Ordering::SeqCst);
            tree_kill(&tracker.collect_pids());
            register_hung(
                hung_watch,
                HungWatch {
                    nonce: nonce.as_str().to_string(),
                    tool: tool_name,
                    lane: Lane::Stateless,
                    started_at,
                    cancelled_at: now_millis(),
                    finished: finished.clone(),
                },
            );
            Err(DispatchError {
                error: GrammarMcpError::ThreadTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce.to_nonce()),
            })
        }
    }
}

pub(crate) async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<InteractEngine>>>,
    env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    hung_watch: &HungRegistry,
    nonce: &datum::NoncePair,
    source: String,
    args_json: serde_json::Value,
    timeout_ms: Option<u64>,
) -> Result<DispatchOutcome, DispatchError> {
    let log_dir = cache_dir(CacheKind::Interacts, nonce);
    fs::create_dir_all(&log_dir).map_err(|e| DispatchError {
        error: GrammarMcpError::Internal {
            phase: "dispatch_interact::create_log_dir".to_string(),
            reason: format!("create_dir_all {}: {e}", log_dir.display()),
        },
        nonce: None,
    })?;
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
        nonce,
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
        key: nonce.as_str().to_string(),
    };
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let eval_fut = engine.eval(log_dir, source, cancel.clone(), tracker.clone(), finished.clone());
    let timed = tk::timeout(tk::TkDuration::from_millis(effective_timeout), eval_fut).await;
    match timed {
        Ok(Ok(result)) => Ok(DispatchOutcome { nonce: nonce.to_nonce(), result }),
        Ok(Err(failure)) => Err(DispatchError {
            error: failure.into_error(),
            nonce: Some(nonce.to_nonce()),
        }),
        Err(_) => {
            cancel.store(true, Ordering::SeqCst);
            tree_kill(&tracker.collect_pids());
            register_hung(
                hung_watch,
                HungWatch {
                    nonce: nonce.as_str().to_string(),
                    tool: "interact",
                    lane: Lane::Interact,
                    started_at,
                    cancelled_at: now_millis(),
                    finished: finished.clone(),
                },
            );
            Err(DispatchError {
                error: GrammarMcpError::ThreadTimeout {
                    timeout_ms: effective_timeout,
                },
                nonce: Some(nonce.to_nonce()),
            })
        }
    }
}

async fn register_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    nonce: &datum::NoncePair,
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
        nonce.as_str().to_string(),
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

/// Clean-shutdown teardown: cancel every in-flight eval and reap its tree.
pub(crate) async fn teardown_all_in_flight(
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>,
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
    sweep_env_jobs(env_jobs);
    kill_plugin_subprocesses();
}

/// Kill every background job in the host-owned table and reap their trees.
pub(crate) fn sweep_env_jobs(env_jobs: &Arc<std::sync::Mutex<nu::Jobs>>) {
    let tracked: Vec<u32> = {
        let mut jobs = env_jobs.lock().unwrap_or_else(|e| e.into_inner());
        let tracked = jobs
            .iter()
            .filter_map(|(_, job)| match job {
                nu::Job::Thread(thread_job) => Some(thread_job.collect_pids()),
                _ => None,
            })
            .flatten()
            .collect();
        let _ = jobs.kill_all();
        tracked
    };
    tree_kill(&tracked);
}
