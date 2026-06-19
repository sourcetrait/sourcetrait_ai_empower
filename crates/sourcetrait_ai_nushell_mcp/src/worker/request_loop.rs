use crate::*;

/// What: the worker's request-handling loop. Acquires stdin + stdout
/// locks for the worker's lifetime, then loops: read one length-
/// prefixed msgpack frame, decode as RunRequest, call eval_source
/// inside `catch_unwind` (so a panic during eval doesn't kill the
/// worker), and write back a RunResponse. Returns Ok on
/// `UnexpectedEof` (clean shutdown when host closes stdin).
///
/// Why: holding the stdin/stdout locks for the loop's lifetime is
/// safe because the worker is single-threaded; the `catch_unwind`
/// wrapping each eval lets one bad closure body fail one call
/// without bringing down the whole worker. Malformed RunRequests
/// also return a structured error instead of crashing.
///
/// Where: called once from `worker::run::run_worker` right after the
/// worker writes its Hello frame. Owns the entire worker process's
/// execution until the host closes the channel.
pub(crate) fn serve(warm_base: &mut WarmBase) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();
    loop {
        let frame = match read_frame(&mut stdin_lock) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let req: RunRequest = match msgpack::from_slice(&frame) {
            Ok(r) => r,
            Err(e) => {
                let resp = RunResponse {
                    id: 0,
                    ok: false,
                    value: Vec::new(),
                    error: Some(format!("malformed RunRequest: {e}")),
                };
                let bytes = msgpack::to_vec_named(&resp).expect("RunResponse always serializes");
                write_frame(&mut stdout_lock, &bytes)?;
                continue;
            }
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            eval_source(warm_base, &req.log_dir, &req.source)
        }));
        let resp = match outcome {
            Ok(Ok(value_bytes)) => RunResponse {
                id: req.id,
                ok: true,
                value: value_bytes,
                error: None,
            },
            Ok(Err(msg)) => RunResponse {
                id: req.id,
                ok: false,
                value: Vec::new(),
                error: Some(msg),
            },
            Err(_) => RunResponse {
                id: req.id,
                ok: false,
                value: Vec::new(),
                error: Some("worker panic during eval (caught)".to_string()),
            },
        };
        let bytes = msgpack::to_vec_named(&resp).expect("RunResponse always serializes");
        write_frame(&mut stdout_lock, &bytes)?;
    }
}

/// What: parses and evaluates `source` against the worker's
/// `warm_base.engine_state`. Opens `<log_dir>/{stdout,stderr}` files
/// and routes external commands' output to them via
/// `Stack::stdout_file`/`stderr_file`. Branches on `warm_base.mode`:
/// stateless clones engine_state per call, stateful mutates the
/// persistent state. On success, converts the result Value to JSON
/// via `nu_json` and msgpack-encodes the bytes.
///
/// Why: the engine-layer file redirect keeps external command
/// chatter from corrupting the worker's fd 1 (which serve() uses
/// exclusively for IPC). The mode-branch matches run()'s stateless
/// vs interact()'s stateful contracts. The JSON conversion at the
/// end lets the agent see structured data; nu_json::Value::from_value
/// gives the same shape as nushell's `to json` command.
///
/// Where: called by `serve` inside `catch_unwind` for each
/// RunRequest; the returned `Vec<u8>` becomes `RunResponse.value`,
/// the Err(msg) becomes `RunResponse.error`.
fn eval_source(
    warm_base: &mut WarmBase,
    log_dir: &std::path::Path,
    source: &str,
) -> Result<Vec<u8>, String> {
    // Redirect external command stdout/stderr at the engine layer so they
    // never reach the worker process's fd 1, which `serve` above uses
    // exclusively for length-prefixed msgpack IPC frames. Without this, a
    // bare `^cmd` in a closure body would corrupt the IPC byte stream and
    // hang the host on its next response read. The host has already
    // created `log_dir` before dispatching this RunRequest; the engine
    // writes external chatter into `<log_dir>/stdout` and `<log_dir>/stderr`
    // so the agent can fetch it by nonce out of band.
    let stdout_file = fs::File::create(log_dir.join("stdout"))
        .map_err(|e| format!("open {}/stdout: {e}", log_dir.display()))?;
    let stderr_file = fs::File::create(log_dir.join("stderr"))
        .map_err(|e| format!("open {}/stderr: {e}", log_dir.display()))?;
    let mut stack = nu::Stack::new()
        .stdout_file(stdout_file)
        .stderr_file(stderr_file)
        .capture_all();
    // Stateless: clone engine_state per call so merge_delta side effects
    // disappear when the local clone is dropped. Stateful: eval against
    // warm_base.engine_state directly, then merge the call's Stack back so
    // env mutations and cd survive.
    let mut local_clone: Option<nu::EngineState> = match warm_base.mode {
        Mode::Stateless => Some(warm_base.engine_state.clone()),
        Mode::Stateful => None,
    };
    let engine_state: &mut nu::EngineState = match &mut local_clone {
        Some(es) => es,
        None => &mut warm_base.engine_state,
    };
    engine_state.set_signals(nu::Signals::new(Arc::new(AtomicBool::new(false))));
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
    // Stateful mode: merge env mutations from the call's Stack into the
    // persistent engine_state so $env.X = ... and `cd` survive into the
    // next interact() call. Stateless mode skips this -- the local clone
    // is about to be dropped anyway.
    if matches!(warm_base.mode, Mode::Stateful) {
        // $env.NONCE is per-call ambient context, not session state: drop it
        // from the stack before merging so it never persists into the next
        // interact() call. The body already read it during eval above; the
        // body's own $env writes are untouched and still merge through.
        let _ = stack.remove_env_var(engine_state, "NONCE");
        engine_state
            .merge_env(&mut stack)
            .map_err(|e| format!("merge_env: {e}"))?;
    }
    // Convert the nushell Value to a JSON value via nu_json (the same
    // converter the `to json` command uses, so the semantics match what
    // a nushell user would see). Per the_user 2026-05-31 design call:
    // closures and ranges aren't passed back, so the JSON-lossy nushell
    // types we'd otherwise need NUON to preserve are out of contract.
    // Structured JSON in the envelope's `result` field beats a quoted
    // NUON string for agent ergonomics.
    let json_value = nu::JsonValue::from_value(value).map_err(|e| format!("Value to JSON: {e}"))?;
    msgpack::to_vec_named(&json_value).map_err(|e| format!("msgpack value: {e}"))
}
