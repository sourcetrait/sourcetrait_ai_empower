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
    pub body: String,
    /// Optional per-call timeout in milliseconds; defaults to 120000 (2 minutes). The usage is cancelled if it exceeds this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, ser::Deserialize, ser::Serialize)]
pub(crate) struct ClosureCacheBody {
    pub(crate) args_type: String,
    pub(crate) result_type: String,
    pub(crate) body: String,
}

pub struct NuSh {
    pub(crate) runs_pool: Arc<Pool>,
    pub(crate) interact_worker: Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
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
    pub pid: u32,
    pub kind: InFlightKind,
}

pub(crate) enum InFlightKind {
    Run,
    Interact,
    Rerun { rerun_id: String },
    Call { path: String },
}

impl NuSh {
    pub(crate) fn new(
        runs_pool: Arc<Pool>,
        interact_worker: Option<WorkerHandle>,
        nonce_gen: Arc<NonceGen>,
        library_locks: Arc<LibraryLocks>,
        lint_engine: Arc<ParseEngine>,
    ) -> Self {
        Self {
            runs_pool,
            interact_worker: Arc::new(tk::AsyncMutex::new(interact_worker)),
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
    pool: &Arc<Pool>,
    in_flight: &Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    log_kind: CacheKind,
    nonce: Nonce,
    source: String,
    tool_name: &'static str,
    args_json: serde_json::Value,
    kind: InFlightKind,
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

pub(crate) async fn dispatch_interact(
    interact: &Arc<tk::AsyncMutex<Option<WorkerHandle>>>,
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


