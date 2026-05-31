use crate::*;

/// What: the host binary's entry point. Builds a fresh tokio runtime,
/// runs the slice-3 substrate (keypair + libraries repo + lock
/// registry) to completion, spawns both workers (stateless and
/// stateful) in parallel, constructs the `NuSh` rmcp server, and
/// blocks on `service.waiting()` until the rmcp connection closes.
/// Side effects: writes `$XDG_DATA_HOME/nu_sh_mcp/{keypair,libraries}`
/// on first startup; binds stdin/stdout for MCP JSON-RPC framing via
/// `mcp::stdio()`.
///
/// Why: pulling the runtime + substrate + spawn + serve choreography
/// into a single pub fn lets `src/main.rs` stay a one-liner. Running
/// substrate BEFORE workers spawn means a substrate failure (e.g.
/// bad keypair perms) surfaces cleanly instead of leaving orphan
/// worker processes alive.
///
/// Where: called from `src/main.rs::main`. Not invoked anywhere else;
/// it owns the server process's main thread.
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
        // Slice 5.1 substrate: a full-shell `ParseEngine` shared across
        // every body-lint pass. The slice 4.x function-file validator's
        // lang-only ParseEngine is still constructed per-invocation
        // inside `library::validate_library_source`; the lint variant
        // gets its own field because the regex-receiver skip set
        // requires the full shell decl table (slice 5.0 probe).
        let lint_engine = Arc::new(ParseEngine::new_full());
        let server = NuSh::new(
            runs_worker,
            interact_worker,
            nonce_gen,
            library_locks,
            lint_engine,
        );
        let service = server.serve(mcp::stdio()).await.expect("serve stdio");
        service.waiting().await.expect("service waiting");
    });
}
