use crate::*;

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
/// Where: called from `src/bin/nu_sh_mcp_worker.rs::main` after
/// parsing the `--mode` CLI flag and translating CliMode -> Mode.
pub fn run_worker(mode: Mode) {
    let mut warm_base = WarmBase::new(mode);
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let hello = Hello { protocol_version: PROTOCOL_VERSION };
    let hello_bytes = msgpack::to_vec_named(&hello)
        .expect("Hello serializes");
    if let Err(e) = write_frame(&mut stdout_lock, &hello_bytes) {
        eprintln!("nu_sh_mcp_worker: failed to write Hello: {e}");
        process::exit(1);
    }
    drop(stdout_lock);
    match worker::request_loop::serve(&mut warm_base) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("nu_sh_mcp_worker: {e}");
            process::exit(1);
        }
    }
}
