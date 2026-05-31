use crate::*;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::AtomicBool;

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
            eval_source(warm_base, &req.source)
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

fn eval_source(warm_base: &WarmBase, source: &str) -> Result<Vec<u8>, String> {
    let mut engine_state = warm_base.engine_state.clone();
    engine_state.set_signals(nu::Signals::new(Arc::new(AtomicBool::new(false))));
    let mut stack = nu::Stack::new().capture_all();
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
    let nuon_str = nu::to_nuon(&engine_state, &value, nu::ToNuonConfig::default())
        .map_err(|e| format!("to_nuon: {e}"))?;
    msgpack::to_vec_named(&nuon_str).map_err(|e| format!("msgpack value: {e}"))
}
