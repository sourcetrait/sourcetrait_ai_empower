use crate::*;

pub(crate) async fn run_server() {
    // Become the subreaper so an eval's orphaned grandchildren stay on our /proc
    // ppid chain for the tree-kill (server/teardown.rs).
    install_child_subreaper();
    let library_locks = ensure_substrate().await.expect("ensure_substrate");
    let nonce_gen = Arc::new(NonceGen::new());
    let lint_engine = Arc::new(ParseEngine::new_full());
    let server = NuSh::new(nonce_gen, library_locks, lint_engine);
    let in_flight = server.in_flight.clone();
    let service = server.serve(mcp::stdio()).await.expect("serve stdio");
    service.waiting().await.expect("service waiting");
    // Clean-shutdown teardown: the client closed stdin; cancel + reap any eval
    // still in flight so a disconnect mid-eval leaks no process tree.
    teardown_all_in_flight(&in_flight).await;
}

/// Max concurrent in-process evals. `available_parallelism / 2` (min 1): eval
/// runs on a dedicated blocking thread and the host itself needs cores for the
/// async runtime + the supervisor; the worker-era `- 3` reserved cores for the
/// interact-worker + reaper PROCESSES, both now deleted.
pub(crate) fn eval_concurrency_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(1)
}
