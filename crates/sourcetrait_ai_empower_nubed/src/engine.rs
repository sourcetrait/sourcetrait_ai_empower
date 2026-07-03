use crate::*;

/// An isolated nushell embedding engine for purely functional filter scripts:
/// a value goes in, a value comes out, and nothing else is touched.
///
/// Fully configured through `NuBedConfig`. The command surface is the curated
/// allowlist in `commands.rs` - no filesystem, no externals, no network, no
/// process control. Every run is hermetic: a fresh engine clone plus a fresh
/// stack, so defs and `$env` writes from one run never reach the next, and
/// nothing ever merges back into the host.
pub struct NuBed {
    base: nu::EngineState,
    config: NuBedConfig,
}

impl NuBed {
    /// Build an engine from its configuration (the curated command context is
    /// constructed once here; runs clone it).
    pub fn new(config: NuBedConfig) -> NuBedResult<Self> {
        Ok(Self {
            base: base_engine_state()?,
            config,
        })
    }

    /// Run a filter script file (UTF-8, BOM tolerated). See `run_script` for
    /// the input/args contract; the file path becomes the script name in
    /// errors.
    pub fn run_script_file(
        &self,
        path: &Path,
        input: Option<Value>,
        args: &[Value],
    ) -> NuBedResult<Value> {
        let source = fs::read_to_string(path).context(ScriptReadSnafu { path: path.to_path_buf() })?;
        self.run_script(&source, &path.display().to_string(), input, args)
    }

    /// Run filter script source. The two data channels:
    ///
    /// - If the script defines `main` (`def` or `export def`): the top-level
    ///   block evaluates first (no input; defs / consts / setup) - a top-level
    ///   `return` short-circuits with its value - then `main` is invoked with
    ///   `args` bound to its positionals (nu's own signature machinery:
    ///   runtime type checks, optionals, defaults, rest) and `input` as its
    ///   pipeline input (`$in`).
    /// - If there is no `main`: `args` must be empty (else
    ///   `NuBedError::MissingMain`) and `input` feeds the block's first
    ///   pipeline (`$in`).
    ///
    /// The run's final value is collected and returned; an input of `None` is
    /// an empty pipeline, and a script with no output returns `nothing`.
    pub fn run_script(
        &self,
        source: &str,
        name: &str,
        input: Option<Value>,
        args: &[Value],
    ) -> NuBedResult<Value> {
        let source = source.trim_start_matches('\u{feff}').to_owned();
        let script_name = name.to_owned();
        let engine = self.base.clone();
        let env = self.config.env.clone();
        let args = args.to_vec();
        let interrupt = Arc::new(AtomicBool::new(false));

        // The eval always runs on a worker thread: a nushell panic unwinds
        // the worker (surfacing Panicked) instead of the host, and a timeout
        // can cooperatively interrupt via the engine signals.
        let worker_interrupt = interrupt.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = sender.send(run_in_engine(
                engine,
                worker_interrupt,
                env,
                &source,
                &script_name,
                input,
                args,
            ));
        });

        match self.config.timeout {
            None => match receiver.recv() {
                Ok(result) => {
                    let _ = worker.join();
                    result
                }
                Err(_) => Err(join_panic(worker, name)),
            },
            Some(timeout) => match receiver.recv_timeout(timeout) {
                Ok(result) => {
                    let _ = worker.join();
                    result
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    interrupt.store(true, Ordering::Relaxed);
                    match receiver.recv() {
                        // Completion raced the timeout: keep the real result.
                        Ok(Ok(value)) => {
                            let _ = worker.join();
                            Ok(value)
                        }
                        Ok(Err(_)) => {
                            let _ = worker.join();
                            TimeoutSnafu { name, timeout }.fail()
                        }
                        Err(_) => Err(join_panic(worker, name)),
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => Err(join_panic(worker, name)),
            },
        }
    }
}

/// The whole per-run pipeline, on the worker thread: wire signals, parse +
/// merge the script, seed the hermetic stack, then route by the main/args
/// contract.
fn run_in_engine(
    mut engine: nu::EngineState,
    interrupt: Arc<AtomicBool>,
    env: Vec<(String, Value)>,
    source: &str,
    name: &str,
    input: Option<Value>,
    args: Vec<Value>,
) -> NuBedResult<Value> {
    engine.set_signals(nu::Signals::new(interrupt));

    let block = parse_source(&mut engine, source.as_bytes(), name)?;

    // The hermetic stack: both out streams captured (a stray engine write can
    // never reach the host's stdout - which may be a protocol channel), env
    // seeded ONLY from the config, PWD a neutral root for the lazy cwd reads
    // some pure commands make.
    let mut stack = nu::Stack::new().capture_all();
    stack.add_env_var("PWD".to_string(), Value::string("/", nu::Span::unknown()));
    for (key, value) in env {
        stack.add_env_var(key, value);
    }

    let input = match input {
        Some(value) => nu::PipelineData::Value(value, None),
        None => nu::PipelineData::empty(),
    };

    if engine.find_decl(b"main", &[]).is_some() {
        // Script-file semantics (mirrors nu itself): the top-level block runs
        // first with no input; a top-level early return short-circuits main.
        match nu::eval_block::<nu::WithoutDebug>(&engine, &mut stack, &block, nu::PipelineData::empty())
        {
            Ok(data) => {
                collect(data.body, name)?;
            }
            Err(nu::ShellError::Return { value, .. }) => return Ok(*value),
            Err(error) => return Err(eval_error(name, error)),
        }
        run_main(&mut engine, &mut stack, name, input, args)
    } else if !args.is_empty() {
        MissingMainSnafu { name, count: args.len() }.fail()
    } else {
        let data =
            nu::eval_block_with_early_return::<nu::WithoutDebug>(&engine, &mut stack, &block, input)
                .map_err(|error| eval_error(name, error))?;
        collect(data.body, name)
    }
}

/// Invoke the script's `main` with `args` as positionals and `input` as its
/// pipeline input. Each arg is bound to a synthesized variable and a
/// `main $__nubed_arg_0 ...` call is parsed against the merged engine, so
/// nu's own call machinery does the binding (runtime type checks, optionals,
/// defaults, rest) with no value quoting involved.
fn run_main(
    engine: &mut nu::EngineState,
    stack: &mut nu::Stack,
    name: &str,
    input: nu::PipelineData,
    args: Vec<Value>,
) -> NuBedResult<Value> {
    let mut call_source = String::from("main");
    let mut bindings: Vec<(nu::VarId, Value)> = Vec::new();
    let (block, delta) = {
        let mut working_set = nu::StateWorkingSet::new(engine);
        for (index, value) in args.into_iter().enumerate() {
            let var_name = format!("$__nubed_arg_{index}");
            let var_id = working_set.add_variable(
                var_name.clone().into_bytes(),
                nu::Span::unknown(),
                nu::Type::Any,
                false,
            );
            call_source.push(' ');
            call_source.push_str(&var_name);
            bindings.push((var_id, value));
        }
        let block = nu::parse(
            &mut working_set,
            Some("<nubed main call>"),
            call_source.as_bytes(),
            false,
        );
        check_working_set(&working_set, name)?;
        (block, working_set.render())
    };
    engine
        .merge_delta(delta)
        .map_err(|error| eval_error(name, error))?;
    for (var_id, value) in bindings {
        stack.add_var(var_id, value);
    }
    let data = nu::eval_block_with_early_return::<nu::WithoutDebug>(engine, stack, &block, input)
        .map_err(|error| eval_error(name, error))?;
    collect(data.body, name)
}

/// Parse script source against the engine and merge the delta (mandatory
/// before eval); parse and compile errors surface as their own phases.
fn parse_source(
    engine: &mut nu::EngineState,
    source: &[u8],
    name: &str,
) -> NuBedResult<Arc<nu::Block>> {
    let (block, delta) = {
        let mut working_set = nu::StateWorkingSet::new(engine);
        let block = nu::parse(&mut working_set, Some(name), source, false);
        check_working_set(&working_set, name)?;
        (block, working_set.render())
    };
    engine
        .merge_delta(delta)
        .map_err(|error| eval_error(name, error))?;
    Ok(block)
}

/// Surface a working set's parse errors (then compile errors) joined into one
/// message; errors must be read before `render()` drops them.
fn check_working_set(
    working_set: &nu::StateWorkingSet,
    name: &str,
) -> NuBedResult<()> {
    if !working_set.parse_errors.is_empty() {
        let message = join_errors(working_set.parse_errors.iter());
        return ParseSnafu { name, message }.fail();
    }
    if !working_set.compile_errors.is_empty() {
        let message = join_errors(working_set.compile_errors.iter());
        return CompileSnafu { name, message }.fail();
    }
    Ok(())
}

/// Join up to five error Displays with "; " (enough context without a wall).
fn join_errors<E: std::fmt::Display>(errors: impl Iterator<Item = E>) -> String {
    errors
        .take(5)
        .map(|error| error.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Collect a run's pipeline into the returned Value (stream errors propagate).
fn collect(
    data: nu::PipelineData,
    name: &str,
) -> NuBedResult<Value> {
    data.into_value(nu::Span::unknown())
        .map_err(|error| eval_error(name, error))
}

fn eval_error(
    name: &str,
    error: nu::ShellError,
) -> NuBedError {
    EvalSnafu { name, message: error.to_string() }.build()
}

/// Reap a worker whose result channel closed without a value: a panic unwound
/// it (carry the payload), or it ended silently (still an engine fault).
fn join_panic(
    worker: thread::JoinHandle<()>,
    name: &str,
) -> NuBedError {
    let message = match worker.join() {
        Err(payload) => {
            if let Some(text) = payload.downcast_ref::<&str>() {
                (*text).to_string()
            } else if let Some(text) = payload.downcast_ref::<String>() {
                text.clone()
            } else {
                "unknown panic payload".to_string()
            }
        }
        Ok(()) => "worker ended without a result".to_string(),
    };
    PanickedSnafu { name, message }.build()
}
