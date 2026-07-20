// EmbedEngine: in-process nushell evaluation - the host-side replacement for the
// deleted worker's `WarmBase` + `eval_source`. The stateless base is built once
// and held by the `Executor` (server/executor.rs), which hands each eval a
// pre-built clone onto a dedicated 64 MB blocking thread; the stateful interact
// engine is a single long-lived thread. Cancellation rides a per-eval `Signals`
// flag that server/tool/common.rs registers in the resource registry.
use crate::*;

/// Build an engine base, mirroring the worker's `WarmBase::new` minus the
/// process-boundary concerns. `base_context` (shell + extra, is_interactive=false,
/// is_mcp=true) + plugin decls + `$nu.*` + seeded env + the parse-time
/// `$NU_LIB_DIRS` const. The stateful (interact) base additionally layers the
/// `plugin add/rm/list` admin family. `setsid()` is dropped (the host is a child
/// of the MCP client and cannot detach its own tty); the TLS crypto provider is
/// installed once at host startup, not here.
pub(crate) fn build_base(mode: Mode) -> nu::EngineState {
    let mut engine_state = base_context();
    if matches!(mode, Mode::Stateful) {
        engine_state = nu::add_plugin_command_context(engine_state);
    }
    // Principle 1 (intercept host-death): shadow exit/exec with erroring decls so
    // an in-process body cannot terminate the shared host - there is no worker
    // process boundary to absorb it. Must follow the shell context that defines
    // the real ones (last-registered decl wins name resolution).
    shadow_host_fatal_decls(&mut engine_state);
    load_plugin_decls(&mut engine_state);
    engine_state.generate_nu_constant();
    seed_env(&mut engine_state);
    seed_lib_dirs(&mut engine_state);
    engine_state
}

/// Seed the process env into `$env` (externals need `$env.PATH`; bodies read the
/// ambient EQUIP_* trio). PWD is set to the process cwd; NU_LIB_DIRS is excluded
/// so it cannot override the parse-time const set below.
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
    // The EQUIP_* trio a body reads ambiently (who-am-I / where-is-my-work). The
    // worker era carried these as the worker's spawn env; in-process there is no
    // worker, so set them directly from CONFIG - they are not in the host's env.
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

/// Register the canonical libraries dir as the parse-time `$NU_LIB_DIRS` const, so
/// a body's `use rig/<author>/<library>` resolves against the signed store. The
/// host knows `libraries_dir()` directly (no worker spawn-env handoff).
fn seed_lib_dirs(engine_state: &mut nu::EngineState) {
    set_lib_dirs_const(engine_state, &[libraries_dir()]);
}

/// Evaluate a synthesized source string against `engine_state`, redirecting the
/// eval's external stdout/stderr into `<log_dir>/{stdout,stderr}` (fd 1 is the
/// JSON-RPC channel). Returns the body's terminal value as a friendly JSON value.
/// The caller supplies a fresh clone for a stateless eval, or the persistent
/// engine for interact; `persist` merges the body's `$env`/cd back (interact) and
/// strips the per-call `$env.NONCE`. Port of the worker's `eval_source`, minus the
/// IPC/msgpack hop.
pub(crate) fn eval_in_process(
    engine_state: &mut nu::EngineState,
    log_dir: &std::path::Path,
    source: &str,
    cancel: Arc<AtomicBool>,
    persist: bool,
) -> Result<json::Value, String> {
    let stdout_file = fs::File::create(log_dir.join("stdout"))
        .map_err(|e| format!("open {}/stdout: {e}", log_dir.display()))?;
    let stderr_file = fs::File::create(log_dir.join("stderr"))
        .map_err(|e| format!("open {}/stderr: {e}", log_dir.display()))?;
    let mut stack = nu::Stack::new()
        .stdout_file(stdout_file)
        .stderr_file(stderr_file)
        .capture_all();
    // `cancel` becomes this eval's interrupt Signals: kill(nonce) / a timeout
    // flip it and nushell bails at its next check point (server/tool/common.rs).
    engine_state.set_signals(nu::Signals::new(cancel));
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    let block = nu::parse(&mut working_set, None, source.as_bytes(), false);
    if !working_set.parse_errors.is_empty() {
        let msgs: Vec<String> = working_set
            .parse_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("parse errors: {}", msgs.join("; ")));
    }
    if !working_set.compile_errors.is_empty() {
        let msgs: Vec<String> = working_set
            .compile_errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        return Err(format!("compile errors: {}", msgs.join("; ")));
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
        engine_state
            .merge_env(&mut stack)
            .map_err(|e| format!("merge_env: {e}"))?;
    }
    let json_value = nu::JsonValue::from_value(value).map_err(|e| format!("Value to JSON: {e}"))?;
    json::to_value(&json_value).map_err(|e| format!("json value: {e}"))
}

/// Generous per-eval stack. P0.7: nu def-recursion is guarded (recursion_limit),
/// so the only native overflow is pathological parser nesting (~1000-deep clears
/// at 8 MB, ~10000-deep at 64 MB). 64 MB clears any realistic body; the residual
/// (deeper nesting) aborts the whole host (accepted, the locked contract).
pub(crate) const EVAL_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Run one stateless eval on a dedicated, generously-stacked thread (nu eval is
/// synchronous blocking Rust; the `engine` clone drops when it returns,
/// reclaiming its memory), bridging the result to async via a oneshot. A panic is
/// caught. The caller (the `Executor`) hands a pre-built clone that already
/// carries the host-owned environment-wide `env_jobs` (P0.8), so a body's
/// `job spawn` persists + is visible/killable across evals.
///
/// Cancellation rides `cancel`: it becomes this eval's `Signals`, so kill(nonce)
/// / a timeout trigger it and the eval bails at nushell's next check point,
/// dropping its clone + releasing the permit. A WEDGE (a pure-Rust hot loop that
/// never polls Signals) cannot be reached - it runs to its natural end holding
/// the permit (the accepted residual); external children are reaped separately.
pub(crate) async fn eval_stateless(
    mut engine: nu::EngineState,
    cancel: Arc<AtomicBool>,
    log_dir: std::path::PathBuf,
    source: String,
    permit: tk::OwnedSemaphorePermit,
) -> Result<json::Value, String> {
    let (tx, rx) = tk::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .stack_size(EVAL_STACK_SIZE)
        .name("nu-eval".to_string())
        .spawn(move || {
            // The permit rides in the thread: a wedged (uncancellable) eval keeps
            // its bounded slot occupied, staying visible + bounded rather than
            // leaking a thread while freeing the slot for another wedge.
            let _permit = permit;
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                eval_in_process(&mut engine, &log_dir, &source, cancel, false)
            }));
            let result = match outcome {
                Ok(r) => r,
                Err(_) => Err("panic during eval (caught)".to_string()),
            };
            let _ = tx.send(result);
        });
    if let Err(e) = spawned {
        return Err(format!("spawn eval thread: {e}"));
    }
    match rx.await {
        Ok(r) => r,
        Err(_) => Err("eval thread dropped without result".to_string()),
    }
}

/// The persistent stateful interact engine: a dedicated long-lived thread owning
/// one `EngineState` (build_base Stateful, with the plugin-admin family), fed eval
/// requests over a channel and processing them serially. Replaces the stateful
/// worker PROCESS - env/cd persist across calls via merge_env; the thread carries
/// the same generous stack as the stateless executor (P0.7). A caught eval panic
/// is reported and the loop continues (Phase 4 adds engine rebuild-on-panic/poison).
#[derive(Clone)]
pub(crate) struct InteractEngine {
    tx: tk::UnboundedSender<InteractRequest>,
}

struct InteractRequest {
    log_dir: std::path::PathBuf,
    source: String,
    cancel: Arc<AtomicBool>,
    tracker: nu::ThreadJob,
    respond: tk::oneshot::Sender<Result<json::Value, String>>,
}

impl InteractEngine {
    /// Spawn the interact engine thread, injecting the host-owned environment-wide
    /// jobs table into its persistent engine (P0.8: run() + interact share one
    /// `env_jobs`, so a bg `job spawn` is visible/killable across both).
    pub(crate) fn spawn(env_jobs: Arc<std::sync::Mutex<nu::Jobs>>) -> Self {
        let (tx, mut rx) = tk::unbounded_channel::<InteractRequest>();
        // If the thread fails to spawn (effectively impossible), `rx` drops with the
        // closure and every eval() surfaces the closed channel as an error.
        let _ = std::thread::Builder::new()
            .stack_size(EVAL_STACK_SIZE)
            .name("nu-interact".to_string())
            .spawn(move || {
                let mut engine = build_base(Mode::Stateful);
                engine.jobs = env_jobs;
                while let Some(req) = rx.blocking_recv() {
                    // Track this eval's external children (server/teardown.rs) so a
                    // cancel/timeout can reap them; overwritten fresh each eval.
                    engine.current_job.background_thread_job = Some(req.tracker.clone());
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        eval_in_process(&mut engine, &req.log_dir, &req.source, req.cancel.clone(), true)
                    }));
                    let result = match outcome {
                        Ok(r) => r,
                        Err(_) => Err("panic during interact eval (caught)".to_string()),
                    };
                    let _ = req.respond.send(result);
                }
            });
        Self { tx }
    }

    /// Submit one interact eval to the engine thread and await its result. The
    /// thread serializes calls, so the persistent session state stays consistent
    /// without holding a lock across the eval.
    pub(crate) async fn eval(
        &self,
        log_dir: std::path::PathBuf,
        source: String,
        cancel: Arc<AtomicBool>,
        tracker: nu::ThreadJob,
    ) -> Result<json::Value, String> {
        let (respond, rx) = tk::oneshot::channel();
        self.tx
            .send(InteractRequest {
                log_dir,
                source,
                cancel,
                tracker,
                respond,
            })
            .map_err(|_| "interact engine thread is gone".to_string())?;
        match rx.await {
            Ok(r) => r,
            Err(_) => Err("interact engine dropped the response".to_string()),
        }
    }
}
