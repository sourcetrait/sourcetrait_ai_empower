use crate::*;

/// What: the worker binary's main entry point. Parses `--mode` via
/// the shared CLI parser in `crate::cli`, then delegates to
/// `run_worker` for the actual lifecycle. Public so the worker bin
/// files (`nushell_mcp_worker.rs` + `nushell_mcp_test_worker.rs`) can
/// dispatch through it as one-liners.
///
/// Why: factoring the CLI parse out of `run_worker` lets a single
/// shared entry point drive both the prod and the `_test` worker
/// subprocesses -- they share one parse path. `BuildTarget` plays
/// no role in workers (see `crate::build_target`); the worker just
/// reads `Mode` and starts evaluating IPC frames.
///
/// Where: called from `src/bin/nushell_mcp_worker.rs::main` and
/// `src/bin/nushell_mcp_test_worker.rs::main`.
pub fn worker_main() {
    run_worker(parse_worker_mode());
}

/// What: the worker binary's main loop entry point. Builds the
/// `WarmBase` for the given `Mode`, writes the Hello handshake frame
/// to stdout, and enters the request-handling loop in
/// `worker::request_loop::serve`. Exits the process on any IPC error
/// or after a clean EOF.
///
/// Why: the host needs the worker to declare its protocol version
/// before sending any RunRequests; if the host reads a non-Hello
/// first frame, the protocol is broken. Splitting this thin entry
/// from `serve` keeps the request loop testable without the
/// process-exit semantics.
///
/// Where: called from `worker_main` (the public entry the worker bin
/// files dispatch through). Not exposed beyond the crate.
pub(crate) fn run_worker(mode: Mode) {
    let mut warm_base = WarmBase::new(mode);
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let hello = Hello { protocol_version: PROTOCOL_VERSION };
    let hello_bytes = msgpack::to_vec_named(&hello)
        .expect("Hello serializes");
    if let Err(e) = write_frame(&mut stdout_lock, &hello_bytes) {
        eprintln!("nushell_mcp_worker: failed to write Hello: {e}");
        process::exit(1);
    }
    drop(stdout_lock);
    match worker::request_loop::serve(&mut warm_base) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("nushell_mcp_worker: {e}");
            process::exit(1);
        }
    }
}
