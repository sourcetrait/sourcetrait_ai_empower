use crate::*;

pub fn worker_main() {
    run_worker(parse_worker_mode());
}

pub(crate) fn run_worker(mode: Mode) {
    nu::CRYPTO_PROVIDER.default();
    let mut warm_base = WarmBase::new(mode);
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION,
    };
    let hello_bytes = msgpack::to_vec_named(&hello).expect("Hello serializes");
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
