use crate::*;

/// What: host-side handle to one running worker subprocess. Owns the
/// child's stdin + stdout pipes and a per-handle monotonic id
/// counter used to pair requests with responses on the IPC channel.
///
/// Why: the host needs a typed seam over what is otherwise just a
/// `tokio::process::Child` plus two pipes. Owning `next_id` per
/// handle (instead of globally) keeps the wire ids small and
/// per-channel; on Drop the child is killed so a dropped NuSh
/// doesn't leak workers.
///
/// Where: the stateless run/rerun/call substrate is `NuSh.runs_pool:
/// Arc<Pool>` (a pool of these handles, server::pool); the stateful
/// interact worker is `NuSh.interact_worker:
/// Arc<Mutex<Option<WorkerHandle>>>`. Constructed by `spawn`; used by
/// `send_request` for every tool round-trip.
pub(crate) struct WorkerHandle {
    #[allow(dead_code)]
    child: tk::Child,
    stdin: tk::ChildStdin,
    stdout: tk::ChildStdout,
    next_id: AtomicU64,
    pid: u32,
    mode: Mode,
}

impl WorkerHandle {
    /// Process id of the spawned worker. Stable for the handle's life;
    /// snapshotted at spawn so external killers (slice 5.9's
    /// `kill(nonce)` and slice 5.10's timeout) can target the worker
    /// via `nix::sys::signal::kill` WITHOUT having to lock the handle.
    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    /// Mode this worker was spawned with (`Stateless` for the runs pool,
    /// `Stateful` for the interact worker). Pool uses this on respawn.
    #[allow(dead_code)]
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }
}

/// What: send SIGKILL to the worker process identified by `pid`. Fire-
/// and-forget; the worker's serve loop EOF-completes, the host's pending
/// `send_request` returns Err, and the dispatch path cleans up.
///
/// Why: slice 5.9 `kill(nonce)` and slice 5.10 timeout both need to
/// terminate a specific worker WITHOUT holding the `WorkerHandle`'s
/// mutex (the mutex is held by the in-flight call). Targeting by pid
/// via `nix::sys::signal::kill` is the only way to interrupt from
/// outside the borrow. SIGKILL (not SIGINT) is used because plugin
/// subprocesses of the worker also need to die cleanly; SIGINT would
/// only interrupt nushell eval, leaving plugin children running.
///
/// Where: called by `server::tool::NuSh::kill` (slice 5.9) and by the
/// timeout branch of `dispatch_to_worker` (slice 5.10).
pub(crate) fn kill_worker_pid(pid: u32) {
    let _ = sys::kill(sys::Pid::from_raw(pid as i32), sys::Signal::SIGKILL);
}

impl WorkerHandle {
    /// What: spawn the worker binary with `--mode <stateless|stateful>`,
    /// connect its stdin/stdout to host-owned pipes, then read the
    /// worker's Hello frame and verify `protocol_version` matches.
    /// Returns a ready-to-use `WorkerHandle` or an io::Error if any
    /// step fails.
    ///
    /// Why: the Hello handshake fails fast on version skew (e.g. an
    /// older worker binary lingering in `~/.sys/app/bin`); inherit stderr
    /// so worker panics + nu errors surface in the host's stderr
    /// stream for debugging. Mode is set at spawn time, immutable for
    /// the worker's life.
    ///
    /// Where: called twice from `server::run::run_server` (one
    /// stateless, one stateful) via `tk::try_join!` so both spawns
    /// fail-together if either errors.
    pub(crate) async fn spawn(mode: Mode) -> io::Result<Self> {
        let worker_bin = resolve_worker_bin()?;
        let mode_arg = match mode {
            Mode::Stateless => "stateless",
            Mode::Stateful => "stateful",
        };
        let mut child = tk::Command::new(&worker_bin)
            .arg("--mode")
            .arg(mode_arg)
            // Hand the canonical libraries root to the worker so WarmBase can set
            // $env.NU_LIB_DIRS - lets run()/interact() bodies `use <library>
            // <module> ...`. The worker resolves no XDG/store paths itself, so
            // the host (which knows the store coordinate) passes it across the
            // spawn.
            .env("NUSHELL_MCP_LIBRARIES_DIR", libraries_dir())
            // The store coordinate this host serves. seed_env forwards all
            // inherited env into $env, so bodies + committed call-targets read
            // $env.NUSHELL_MCP_ID / $env.NUSHELL_MCP_NAMESPACE ambiently (the
            // who-am-I answer without pid matching). Workers still never read
            // config().
            .env("NUSHELL_MCP_ID", &config().id)
            .env("NUSHELL_MCP_NAMESPACE", &config().namespace)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("worker stdin pipe missing"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("worker stdout pipe missing"))?;
        let hello_bytes = read_frame_async(&mut stdout).await?;
        let hello: Hello = msgpack::from_slice(&hello_bytes)
            .map_err(|e| io::Error::other(format!("decode Hello: {e}")))?;
        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(io::Error::other(format!(
                "worker protocol_version {} does not match host {}",
                hello.protocol_version, PROTOCOL_VERSION,
            )));
        }
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("worker child pid missing"))?;
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: AtomicU64::new(1),
            pid,
            mode,
        })
    }

    /// What: round-trip one RunRequest through the worker. Allocates
    /// the next sequence id, msgpack-encodes the request, writes the
    /// frame, reads the response frame, msgpack-decodes it, validates
    /// the id matches, and returns the `RunResponse`. Mutates only
    /// the internal counter (atomic, lock-free).
    ///
    /// Why: each round-trip needs serial access to the worker's
    /// stdin + stdout because the IPC has no in-flight multiplexing
    /// (one request, one response). The id check catches channel
    /// desync if the host and worker ever fall out of step.
    ///
    /// Where: called by `server::tool::dispatch_to_worker` after
    /// acquiring the `tk::AsyncMutex` around the handle. The handle
    /// is taken `&mut self` so the borrow checker enforces single
    /// access for the round-trip duration.
    pub(crate) async fn send_request(
        &mut self,
        log_dir: PathBuf,
        source: String,
    ) -> io::Result<RunResponse> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RunRequest {
            id,
            log_dir,
            source,
        };
        let request_bytes = msgpack::to_vec_named(&request)
            .map_err(|e| io::Error::other(format!("encode RunRequest: {e}")))?;
        write_frame_async(&mut self.stdin, &request_bytes).await?;
        let response_bytes = read_frame_async(&mut self.stdout).await?;
        let response: RunResponse = msgpack::from_slice(&response_bytes)
            .map_err(|e| io::Error::other(format!("decode RunResponse: {e}")))?;
        if response.id != id {
            return Err(io::Error::other(format!(
                "RunResponse id {} does not match RunRequest id {}",
                response.id, id,
            )));
        }
        Ok(response)
    }
}

/// What: locate the worker binary on disk. Honors the
/// `NUSHELL_MCP_WORKER_PATH` env var when set; otherwise resolves to a
/// sibling `nushell_mcp_worker` alongside the current executable.
///
/// Why: the integration test suite uses `CARGO_BIN_EXE_*` env var to
/// point each test at a deterministic worker binary path
/// (test-specific, not the installed one); production runs use the
/// sibling-of-current-exe path that `cargo install` produces.
///
/// Where: called by `WorkerHandle::spawn` to find the binary to
/// launch. Not exposed to other modules.
fn resolve_worker_bin() -> io::Result<PathBuf> {
    if let Ok(p) = std::env::var("NUSHELL_MCP_WORKER_PATH") {
        return Ok(PathBuf::from(p));
    }
    let current = std::env::current_exe()?;
    let dir = current
        .parent()
        .ok_or_else(|| io::Error::other("current_exe has no parent directory"))?;
    let host_name = current
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("current_exe basename not utf-8"))?;
    let worker_name = format!("{host_name}_worker");
    Ok(dir.join(&worker_name))
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<WorkerHandle>();
};

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
