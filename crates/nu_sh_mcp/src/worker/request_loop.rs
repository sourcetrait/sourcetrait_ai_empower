use crate::*;

pub(crate) fn serve(warm_base: &WarmBase) -> io::Result<()> {
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
                let bytes = msgpack::to_vec_named(&resp)
                    .expect("RunResponse always serializes");
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
        let bytes = msgpack::to_vec_named(&resp)
            .expect("RunResponse always serializes");
        write_frame(&mut stdout_lock, &bytes)?;
    }
}

fn eval_source(
    warm_base: &WarmBase,
    log_dir: &std::path::Path,
    source: &str,
) -> Result<Vec<u8>, String> {
    let mut engine_state = warm_base.engine_state.clone();
    engine_state.set_signals(nu::Signals::new(Arc::new(AtomicBool::new(false))));
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
    let mut working_set = nu::StateWorkingSet::new(&engine_state);
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
    engine_state.merge_delta(delta).map_err(|e| format!("merge_delta: {e}"))?;
    let pipeline = nu::eval_block::<nu::WithoutDebug>(
        &engine_state,
        &mut stack,
        &block,
        nu::PipelineData::Empty,
    )
    .map_err(|e| format!("eval: {e}"))?;
    let value = pipeline
        .body
        .into_value(nu::Span::unknown())
        .map_err(|e| format!("into_value: {e}"))?;
    // Convert the nushell Value to a JSON value via nu_json (the same
    // converter the `to json` command uses, so the semantics match what
    // a nushell user would see). Per the_user 2026-05-31 design call:
    // closures and ranges aren't passed back, so the JSON-lossy nushell
    // types we'd otherwise need NUON to preserve are out of contract.
    // Structured JSON in the envelope's `result` field beats a quoted
    // NUON string for agent ergonomics.
    let json_value = nu::JsonValue::from_value(value)
        .map_err(|e| format!("Value to JSON: {e}"))?;
    msgpack::to_vec_named(&json_value)
        .map_err(|e| format!("msgpack value: {e}"))
}
