use crate::*;

pub(crate) async fn run_server() {
    // Become the subreaper so an eval's orphaned grandchildren stay on our /proc
    // ppid chain for the tree-kill (server/teardown.rs).
    install_child_subreaper();
    let library_locks = ensure_substrate().await.expect("ensure_substrate");
    let nonce_gen = Arc::new(NonceGen::new());
    let lint_engine = Arc::new(LintEngine::new());
    let server = NuSh::new(nonce_gen, library_locks, lint_engine);
    // The per-process id now belongs to NuSh (info() reports it); the emergency log
    // still namespaces by it (<cache>/log/<mcp_nom>/).
    let mcp_nom = server.mcp_nom.to_string();
    // Emergency lane (Phase 5): the watchdog classifies resource trouble onto an
    // internal channel; one responder appends each Emergency to emergency.nuonl.
    let (emergency_tx, emergency_rx) = tk::unbounded_channel::<Emergency>();
    spawn_emergency_responder(emergency_rx, mcp_nom);
    // The channel classifies spam onto the same lane the watchdog uses. Installed here
    // rather than at construction because the lane does not exist until now; the
    // one-shot CLI runs no responder, so its channel simply has none.
    server.channel.install_emergency(emergency_tx.clone());
    spawn_watchdog(WatchdogDeps {
        hung_watch: server.hung_watch.clone(),
        semaphore: server.executor.semaphore(),
        cap: eval_concurrency_cap(),
        env_jobs: server.env_jobs.clone(),
        tx: emergency_tx,
    });
    // The store's liveness signal, held for the process lifetime. Non-fatal: two hosts
    // may share one store coordinate, and "locked" still answers the watcher's question.
    let _host_lock = match acquire_host_lock(&server.mcp_nom.to_string()) {
        Ok(lock) => Some(lock),
        Err(e) => {
            eprintln!("grammar: host lock not held: {e}");
            None
        }
    };
    let in_flight = server.in_flight.clone();
    let env_jobs = server.env_jobs.clone();
    // TERM / INT / HUP: without these a signalled host drops its eval children and its
    // background jobs onto the box unreaped, because the stdio path only ever learns
    // about a CLEAN client disconnect. None of it reaches SIGKILL - that is what the
    // host lock is for.
    spawn_signal_sweep(in_flight.clone(), env_jobs.clone());
    let service = server.serve(mcp::stdio()).await.expect("serve stdio");
    service.waiting().await.expect("service waiting");
    // Clean-shutdown teardown: the client closed stdin; cancel + reap any eval
    // still in flight so a disconnect mid-eval leaks no process tree.
    close_channel_for_shutdown().await;
    teardown_all_in_flight(&in_flight, &env_jobs).await;
}

/// RFC 6455 "going away" - a server shutting down, which is exactly this.
const CLOSE_GOING_AWAY: u16 = 1001;
const SHUTDOWN_REASON: &str = "host shutting down";

/// How long a shutdown will wait for the close frame to reach the wire. Bounded
/// because a peer that has already gone must never hold the shutdown open.
const CLOSE_FLUSH_GRACE: tk::TkDuration = tk::TkDuration::from_millis(500);

/// Tell the channel's peer we are going away, and wait for the frame to be flushed.
///
/// Without this a shutting-down host simply vanishes and the agent sees a bare 1006,
/// indistinguishable from a crashed host - the very distinction the explicit close
/// frame exists to provide. The frame is written by the hub task, so the wait is an
/// ACK rather than a sleep: a sleep long enough to usually work is the shape of bug
/// this campaign already paid for once.
async fn close_channel_for_shutdown() {
    if let Some(done) = channel_handle().close_and_await(CLOSE_GOING_AWAY, SHUTDOWN_REASON) {
        let _ = tk::timeout(CLOSE_FLUSH_GRACE, done).await;
    }
}

/// Sweep on TERM / INT / HUP, then exit with the conventional `128 + signo`.
///
/// Exiting is part of the contract: handling a termination signal without terminating
/// would make the host ignore the very thing it was told to do.
fn spawn_signal_sweep(
    in_flight: Arc<tk::AsyncMutex<HashMap<String, InFlightEntry>>>,
    env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
) {
    tk::spawn(async move {
        let mut term = match tk::signal(tk::SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("grammar: SIGTERM handler not installed: {e}");
                return;
            }
        };
        let mut interrupt = match tk::signal(tk::SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("grammar: SIGINT handler not installed: {e}");
                return;
            }
        };
        let mut hangup = match tk::signal(tk::SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("grammar: SIGHUP handler not installed: {e}");
                return;
            }
        };
        let signo = tokio::select! {
            _ = term.recv() => nix::sys::signal::Signal::SIGTERM as i32,
            _ = interrupt.recv() => nix::sys::signal::Signal::SIGINT as i32,
            _ = hangup.recv() => nix::sys::signal::Signal::SIGHUP as i32,
        };
        close_channel_for_shutdown().await;
        teardown_all_in_flight(&in_flight, &env_jobs).await;
        process::exit(128 + signo);
    });
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
