use crate::*;

/// What: the host binary's entry point, parameterized on `target`
/// (`BuildTarget::Main` for `nushell_mcp`, `BuildTarget::Test` for
/// `nushell_mcp_test`). Sets `BUILD_TARGET` first so every downstream
/// path helper / serverInfo / validator sees the target. Then builds
/// a fresh tokio runtime, runs the slice-3 substrate (keypair +
/// libraries repo + lock registry) to completion, builds the
/// stateless runs pool (workers spawn lazily on first acquire) +
/// eagerly spawns the single stateful interact worker, constructs the
/// `NuSh` rmcp server, and blocks on `service.waiting()` until the
/// rmcp connection closes. Side effects: writes
/// `$XDG_DATA_HOME/sourcetrait/<target_name>/{keypair,libraries}` on first
/// startup; binds stdin/stdout for MCP JSON-RPC framing via
/// `mcp::stdio()`.
///
/// Why: pulling the runtime + substrate + spawn + serve choreography
/// into a single pub fn lets the binary entries stay one-liners.
/// Setting `BUILD_TARGET` as the very first action keeps the
/// per-binary target available to every reader before any substrate
/// path resolves. Running substrate BEFORE workers spawn means a
/// substrate failure (e.g. bad keypair perms) surfaces cleanly
/// instead of leaving orphan worker processes alive.
///
/// Where: called from `src/main.rs::main` (with `BuildTarget::Main`)
/// and `src/bin/nushell_mcp_test.rs::main` (with `BuildTarget::Test`).
/// Not invoked anywhere else; it owns the server process's main
/// thread.
pub fn run_server(target: BuildTarget) {
    build_target::BUILD_TARGET
        .set(target)
        .expect("BUILD_TARGET set once at startup");
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
        // worker drives interact().
        // Slice 5.8 substrate: pool the stateless `runs` workers. Cap
        // = max(1, available_parallelism - 3) matches the_user
        // 2026-06-01 lock (warm parity); min 1 always available; idle
        // reap at 60s. Interact stays single-worker because stateful
        // sessions can't be sensibly pooled.
        let cap = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(3).max(1))
            .unwrap_or(1);
        let runs_pool = Pool::new(Mode::Stateless, cap, 1, tk::TkDuration::from_secs(60));
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
            interact_worker,
            nonce_gen,
            library_locks,
            lint_engine,
        );
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
