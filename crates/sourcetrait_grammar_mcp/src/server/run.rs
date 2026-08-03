use crate::*;

pub(crate) async fn run_server() {
    install_child_subreaper();
    ensure_cert_profile();
    let rig_locks = ensure_substrate().await.expect("ensure_substrate");
    let nonce_gen = Arc::new(NonceGen::new());
    let lint_engine = Arc::new(LintEngine::new());
    let server = NuSh::new(nonce_gen, rig_locks, lint_engine);
    let mcp_nom = server.mcp_nom.to_string();
    let (emergency_tx, emergency_rx) = tk::unbounded_channel::<Emergency>();
    spawn_emergency_responder(emergency_rx, mcp_nom);
    server.channel.install_emergency(emergency_tx.clone());
    spawn_watchdog(WatchdogDeps {
        hung_watch: server.hung_watch.clone(),
        semaphore: server.executor.semaphore(),
        cap: eval_concurrency_cap(),
        env_jobs: server.env_jobs.clone(),
        tx: emergency_tx,
    });
    let _host_lock = match acquire_host_lock(&server.mcp_nom.to_string()) {
        Ok(lock) => Some(lock),
        Err(e) => {
            eprintln!("grammar: host lock not held: {e}");
            None
        }
    };
    let in_flight = server.in_flight.clone();
    let env_jobs = server.env_jobs.clone();
    spawn_signal_sweep(in_flight.clone(), env_jobs.clone());
    spawn_remote_listener_from_config(server.mcp_nom.to_string(), server.remote_links.clone()).await;
    let service = server.serve(mcp::stdio()).await.expect("serve stdio");
    service.waiting().await.expect("service waiting");
    close_channel_for_shutdown().await;
    teardown_all_in_flight(&in_flight, &env_jobs).await;
}

/// RFC 6455 "going away" - a server shutting down, which is exactly this.
const CLOSE_GOING_AWAY: u16 = 1001;
const SHUTDOWN_REASON: &str = "host shutting down";

/// How long a shutdown waits for the close frame to reach the wire.
const CLOSE_FLUSH_GRACE: tk::TkDuration = tk::TkDuration::from_millis(500);

/// Tell the channel's peer we are going away, and wait for the flush.
async fn close_channel_for_shutdown() {
    if let Some(done) = channel_handle().close_and_await(CLOSE_GOING_AWAY, SHUTDOWN_REASON) {
        let _ = tk::timeout(CLOSE_FLUSH_GRACE, done).await;
    }
}

/// Sweep on TERM / INT / HUP, then exit with the conventional `128 + signo`.
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

/// Max concurrent in-process evals: available parallelism halved, min 1.
pub(crate) fn eval_concurrency_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(1)
}
