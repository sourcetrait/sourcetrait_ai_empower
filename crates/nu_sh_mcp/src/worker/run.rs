use crate::*;

pub fn run_worker() {
    let warm_base = WarmBase::new();
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
    match worker::request_loop::serve(&warm_base) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("nu_sh_mcp_worker: {e}");
            process::exit(1);
        }
    }
}
