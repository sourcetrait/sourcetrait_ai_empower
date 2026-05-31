use crate::*;

pub fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
        // Run the one-time-per-startup substrate: keypair gen, libraries
        // git repo init, per-library lock registry hydration from disk.
        // Idempotent; safe on every startup. Happens BEFORE workers
        // spawn so a substrate failure surfaces cleanly without leaving
        // worker processes orphaned.
        let library_locks = ensure_substrate().await.expect("ensure_substrate");
        // Spawn the stateless and stateful workers in parallel; tk::try_join!
        // gives fail-together semantics -- if either spawn (or its Hello
        // handshake) errors, the server refuses to start. Both worker
        // processes are needed: the stateless one drives run(), the
        // stateful one drives interact().
        let (runs_worker, interact_worker) = tk::try_join!(
            WorkerHandle::spawn(Mode::Stateless),
            WorkerHandle::spawn(Mode::Stateful),
        )
        .expect("spawn both workers");
        let nonce_gen = Arc::new(lib_empower::NonceGen::new());
        let server = NuSh::new(runs_worker, interact_worker, nonce_gen, library_locks);
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
