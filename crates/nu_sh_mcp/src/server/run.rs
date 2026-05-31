use crate::*;

pub fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
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
        let server = NuSh::new(runs_worker, interact_worker, nonce_gen);
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
