use crate::*;

pub(crate) fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
        let library_locks = ensure_substrate().await.expect("ensure_substrate");
        let runs_pool = Pool::new(
            Mode::Stateless,
            worker_pool_cap(),
            1,
            tk::TkDuration::from_secs(60),
        );
        let interact_worker = WorkerHandle::spawn(Mode::Stateful)
            .await
            .expect("spawn interact worker");
        let nonce_gen = Arc::new(NonceGen::new());
        let lint_engine = Arc::new(ParseEngine::new_full());
        let server = NuSh::new(
            runs_pool,
            Some(interact_worker),
            nonce_gen,
            library_locks,
            lint_engine,
        );
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}

pub(crate) fn worker_pool_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(3).max(1))
        .unwrap_or(1)
}
