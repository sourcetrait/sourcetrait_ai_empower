use crate::*;

/// What: the MCP serve path. Builds a fresh tokio runtime, runs the
/// substrate (keypair + libraries repo + lock hydration) to completion,
/// builds the stateless runs pool (workers spawn lazily on first acquire)
/// + eagerly spawns the single stateful interact worker, constructs the
/// `NuSh` rmcp server, and blocks on `service.waiting()` until the rmcp
/// connection closes. Side effects: writes
/// `$XDG_DATA_HOME/sourcetrait/nushell_mcp/<id>/<namespace>/
/// {keypair,libraries}` on first startup; binds stdin/stdout for MCP
/// JSON-RPC framing via `mcp::stdio()`.
///
/// Why: pulling the runtime + substrate + spawn + serve choreography into
/// one fn keeps `cli::host_main` a dispatcher. `CONFIG` is already stored
/// (host_main's first action), so every path helper below reads the
/// store coordinate. Running substrate BEFORE workers spawn means a
/// substrate failure (e.g. bad keypair perms) surfaces cleanly instead
/// of leaving orphan worker processes alive.
///
/// Where: called by `cli::host_main` when no subcommand was given (the
/// `.mcp.json` server-entry path).
pub(crate) fn run_server() {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    rt.block_on(async {
        // Run the one-time-per-startup substrate: keypair gen, libraries
        // git repo init, per-library lock registry hydration from disk.
        // Idempotent; safe on every startup. Happens BEFORE workers
        // spawn so a substrate failure surfaces cleanly without leaving
        // worker processes orphaned.
        let library_locks = ensure_substrate().await.expect("ensure_substrate");
        // Build the stateless runs pool (lazy: no worker spawns at
        // construction; each spawns on first acquire) and eagerly spawn
        // the single stateful interact worker. Both substrates are
        // needed: the pool drives run() / rerun() / call(), the interact
        // worker drives interact(). Interact stays single-worker because
        // stateful sessions can't be sensibly pooled.
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
        // A full-shell `ParseEngine` shared by BOTH the body-lint pass
        // (`engine_state()`) and the library validator
        // (`engine_state_for_file()`, threaded through commit_impl /
        // check_library / install_impl). Full shell (not lang-only) so the
        // lint's regex-receiver skip set resolves `str replace --regex` et al.
        // as Calls (slice 5.0 probe); the validator clones it per file to
        // layer $env.PWD + NU_LIB_DIRS for module resolution (parse_engine.rs).
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

/// What: the stateless-pool capacity: max(1, available_parallelism - 3).
///
/// Why: the_user 2026-06-01 lock (warm parity) -- leave headroom for the
/// host, the interact worker, and the client; min 1 keeps the pool
/// usable on tiny machines. Shared by the serve path and the one-shot
/// CLI driver so both build the same pool shape.
///
/// Where: called by `run_server` and `server::oneshot::run_oneshot`.
pub(crate) fn worker_pool_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(3).max(1))
        .unwrap_or(1)
}
