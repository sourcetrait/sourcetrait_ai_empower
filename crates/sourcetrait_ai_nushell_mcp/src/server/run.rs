use crate::*;

pub(crate) async fn run_server() {
    let library_locks = ensure_substrate().await.expect("ensure_substrate");
    let nonce_gen = Arc::new(NonceGen::new());
    let lint_engine = Arc::new(ParseEngine::new_full());
    let server = NuSh::new(nonce_gen, library_locks, lint_engine);
    let service = server.serve(mcp::stdio()).await.expect("serve stdio");
    service.waiting().await.expect("service waiting");
}

pub(crate) fn worker_pool_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(3).max(1))
        .unwrap_or(1)
}
