//! In-process nushell evaluation: the host-side eval engine.
use crate::*;

/// Flips a liveness flag on drop, so a hung thread is distinguishable.
struct FinishGuard(Arc<AtomicBool>);

impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Why an eval failed, keeping the one distinction the wire needs.
pub(crate) enum EvalFailure {
    Reported(String),
    CircularImport(String),
}

impl From<String> for EvalFailure {
    fn from(reason: String) -> Self {
        Self::Reported(reason)
    }
}

impl EvalFailure {
    pub(crate) fn into_error(self) -> GrammarMcpError {
        match self {
            Self::Reported(reason) => GrammarMcpError::ThreadReturnedError { reason },
            Self::CircularImport(files) => GrammarMcpError::ModuleCircularImport { files },
        }
    }
}

/// Build an engine base for the given mode.
pub(crate) fn build_base(mode: Mode) -> nu::EngineState {
    let mut engine_state = base_context();
    if matches!(mode, Mode::Stateful) {
        engine_state = nu::add_plugin_command_context(engine_state);
    }
    shadow_host_fatal_decls(&mut engine_state);
    load_plugin_decls(&mut engine_state);
    engine_state.generate_nu_constant();
    seed_env(&mut engine_state);
    seed_lib_dirs(&mut engine_state);
    #[cfg(feature = "test-hooks")]
    crate::server::test_hooks::register_test_hooks(&mut engine_state);
    engine_state
}

/// Seed the process env into `$env`, plus the ambient EQUIP trio.
fn seed_env(engine_state: &mut nu::EngineState) {
    if let Ok(cwd) = std::env::current_dir() {
        engine_state.add_env_var(
            "PWD".to_string(),
            nu::Value::string(cwd.to_string_lossy().into_owned(), nu::Span::unknown()),
        );
    }
    for (key, val) in std::env::vars() {
        if key == "PWD" || key == "NU_LIB_DIRS" {
            continue;
        }
        engine_state.add_env_var(key, nu::Value::string(val, nu::Span::unknown()));
    }
    let cfg = config();
    let span = nu::Span::unknown();
    engine_state.add_env_var("EQUIP_ID".to_string(), nu::Value::string(cfg.id.clone(), span));
    engine_state.add_env_var(
        "EQUIP_NAMESPACE".to_string(),
        nu::Value::string(cfg.namespace.clone(), span),
    );
    engine_state.add_env_var(
        "EQUIP_WORK_DIR".to_string(),
        nu::Value::string(cfg.work_dir.to_string_lossy().into_owned(), span),
    );
}

/// Register the canonical rigs dir as the parse-time `$NU_LIB_DIRS` const.
fn seed_lib_dirs(engine_state: &mut nu::EngineState) {
    set_lib_dirs_const(engine_state, &[rigs_dir()]);
}

/// `EngineState::merge_env` minus the process chdir.
fn merge_env_no_chdir(
    engine_state: &mut nu::EngineState,
    stack: &mut nu::Stack,
) {
    for mut scope in stack.env_vars.drain(..) {
        for (overlay_name, mut env) in Arc::make_mut(&mut scope).drain() {
            if let Some(env_vars) =
                Arc::make_mut(&mut engine_state.env_vars).get_mut(&overlay_name)
            {
                env_vars.extend(env.drain());
            } else {
                Arc::make_mut(&mut engine_state.env_vars).insert(overlay_name, env);
            }
        }
    }
    if let Some(config) = stack.config.take() {
        engine_state.set_config(config);
    }
}

/// Evaluate a synthesized source string against `engine_state`.
pub(crate) fn eval_in_process(
    engine_state: &mut nu::EngineState,
    log_dir: &std::path::Path,
    source: &str,
    cancel: Arc<AtomicBool>,
    persist: bool,
) -> Result<json::Value, EvalFailure> {
    let stdout_file = fs::File::create(log_dir.join("stdout"))
        .map_err(|e| format!("open {}/stdout: {e}", log_dir.display()))?;
    let stderr_file = fs::File::create(log_dir.join("stderr"))
        .map_err(|e| format!("open {}/stderr: {e}", log_dir.display()))?;
    let mut stack = nu::Stack::new()
        .stdout_file(stdout_file)
        .stderr_file(stderr_file)
        .capture_all();
    engine_state.set_signals(nu::Signals::new(cancel));
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    register_nuapi(&mut working_set, log_dir);
    let files_before = working_set.num_files();
    let block = nu::parse(&mut working_set, None, source.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
        if let Some(files) = detect_import_cycle(&working_set, files_before) {
            return Err(EvalFailure::CircularImport(files));
        }
        let msgs: Vec<String> = working_set
            .parse_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("parse errors: {}", msgs.join("; ")).into());
    }
    if !working_set.compile_errors.is_empty() {
        let msgs: Vec<String> = working_set
            .compile_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("compile errors: {}", msgs.join("; ")).into());
    }
    let delta = working_set.render();
    engine_state
        .merge_delta(delta)
        .map_err(|e| format!("merge_delta: {e}"))?;
    let pipeline = nu::eval_block::<nu::WithoutDebug>(
        engine_state,
        &mut stack,
        &block,
        nu::PipelineData::Empty,
    )
    .map_err(|e| format!("eval: {e}"))?;
    let value = pipeline
        .body
        .into_value(nu::Span::unknown())
        .map_err(|e| format!("into_value: {e}"))?;
    if persist {
        let _ = stack.remove_env_var(engine_state, "NONCE");
        merge_env_no_chdir(engine_state, &mut stack);
    }
    let json_value = nu::JsonValue::from_value(value).map_err(|e| format!("Value to JSON: {e}"))?;
    json::to_value(&json_value).map_err(|e| format!("json value: {e}").into())
}

/// Generous per-eval stack; the parser is what needs it.
pub(crate) const EVAL_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Run one stateless eval on a dedicated, generously-stacked thread.
pub(crate) async fn eval_stateless(
    mut engine: nu::EngineState,
    cancel: Arc<AtomicBool>,
    log_dir: std::path::PathBuf,
    source: String,
    permit: tk::OwnedSemaphorePermit,
    finished: Arc<AtomicBool>,
) -> Result<json::Value, EvalFailure> {
    let (tx, rx) = tk::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .stack_size(EVAL_STACK_SIZE)
        .name("nu-eval".to_string())
        .spawn(move || {
            let _permit = permit;
            let _finish = FinishGuard(finished);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                eval_in_process(&mut engine, &log_dir, &source, cancel, false)
            }));
            let result = match outcome {
                Ok(r) => r,
                Err(_) => Err("panic during eval (caught)".to_string().into()),
            };
            let _ = tx.send(result);
        });
    if let Err(e) = spawned {
        return Err(format!("spawn eval thread: {e}").into());
    }
    match rx.await {
        Ok(r) => r,
        Err(_) => Err("eval thread dropped without result".to_string().into()),
    }
}

/// The persistent stateful interact engine: one long-lived serial thread.
#[derive(Clone)]
pub(crate) struct InteractEngine {
    tx: tk::UnboundedSender<InteractRequest>,
}

struct InteractRequest {
    log_dir: std::path::PathBuf,
    source: String,
    cancel: Arc<AtomicBool>,
    tracker: nu::ThreadJob,
    finished: Arc<AtomicBool>,
    respond: tk::oneshot::Sender<Result<json::Value, EvalFailure>>,
}

impl InteractEngine {
    /// Spawn the interact engine thread with the host-owned jobs table.
    pub(crate) fn spawn(env_jobs: Arc<std::sync::Mutex<nu::Jobs>>) -> Self {
        let (tx, mut rx) = tk::unbounded_channel::<InteractRequest>();
        let _ = std::thread::Builder::new()
            .stack_size(EVAL_STACK_SIZE)
            .name("nu-interact".to_string())
            .spawn(move || {
                let mut engine = build_base(Mode::Stateful);
                engine.jobs = env_jobs.clone();
                while let Some(req) = rx.blocking_recv() {
                    let _finish = FinishGuard(req.finished.clone());
                    engine.current_job.background_thread_job = Some(req.tracker.clone());
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        eval_in_process(&mut engine, &req.log_dir, &req.source, req.cancel.clone(), true)
                    }));
                    let result = match outcome {
                        Ok(r) => r,
                        Err(_) => {
                            if env_jobs.is_poisoned() {
                                *env_jobs.lock().unwrap_or_else(|e| e.into_inner()) =
                                    nu::Jobs::default();
                                env_jobs.clear_poison();
                            }
                            engine = build_base(Mode::Stateful);
                            engine.jobs = env_jobs.clone();
                            Err("panic during interact eval (caught); interact session reset"
                                .to_string()
                                .into())
                        }
                    };
                    let _ = req.respond.send(result);
                }
            });
        Self { tx }
    }

    /// Submit one interact eval to the engine thread and await its result.
    pub(crate) async fn eval(
        &self,
        log_dir: std::path::PathBuf,
        source: String,
        cancel: Arc<AtomicBool>,
        tracker: nu::ThreadJob,
        finished: Arc<AtomicBool>,
    ) -> Result<json::Value, EvalFailure> {
        let (respond, rx) = tk::oneshot::channel();
        self.tx
            .send(InteractRequest {
                log_dir,
                source,
                cancel,
                tracker,
                finished,
                respond,
            })
            .map_err(|_| "interact engine thread is gone".to_string())?;
        match rx.await {
            Ok(r) => r,
            Err(_) => Err("interact engine dropped the response".to_string().into()),
        }
    }
}
