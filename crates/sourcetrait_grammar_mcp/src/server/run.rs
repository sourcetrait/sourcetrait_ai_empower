use crate::*;

pub(crate) async fn run_server() {
    // Become the subreaper so an eval's orphaned grandchildren stay on our /proc
    // ppid chain for the tree-kill (server/teardown.rs).
    install_child_subreaper();
    let library_locks = ensure_substrate().await.expect("ensure_substrate");
    let nonce_gen = Arc::new(NonceGen::new());
    let lint_engine = Arc::new(ParseEngine::new_full());
    let server = NuSh::new(nonce_gen, library_locks, lint_engine);
    // The per-process id now belongs to NuSh (info() reports it); the emergency log
    // still namespaces by it (<cache>/log/<mcp_nom>/).
    let mcp_nom = server.mcp_nom.to_string();
    // Emergency lane (Phase 5): the watchdog classifies resource trouble onto an
    // internal channel; one responder appends each Emergency to emergency.nuonl.
    let (emergency_tx, emergency_rx) = tk::unbounded_channel::<Emergency>();
    spawn_emergency_responder(emergency_rx, mcp_nom);
    spawn_watchdog(WatchdogDeps {
        hung_watch: server.hung_watch.clone(),
        semaphore: server.executor.semaphore(),
        cap: eval_concurrency_cap(),
        env_jobs: server.env_jobs.clone(),
        tx: emergency_tx,
    });
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
