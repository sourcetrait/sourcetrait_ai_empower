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
/// Where: two instances live in `server::tool::NuSh` (`runs_worker`
/// + `interact_worker`), each guarded by `tokio::sync::Mutex` for
/// serialized access. Constructed by `spawn`; used by `send_request`
/// for every tool round-trip.
pub(crate) struct WorkerHandle {
    #[allow(dead_code)]
    child: tk::Child,
    stdin: tk::ChildStdin,
    stdout: tk::ChildStdout,
    next_id: AtomicU64,
}

impl WorkerHandle {
    /// What: spawn the worker binary with `--mode <stateless|stateful>`,
    /// connect its stdin/stdout to host-owned pipes, then read the
    /// worker's Hello frame and verify `protocol_version` matches.
    /// Returns a ready-to-use `WorkerHandle` or an io::Error if any
    /// step fails.
    ///
    /// Why: the Hello handshake fails fast on version skew (e.g. an
    /// older worker binary lingering in `~/app/bin`); inherit stderr
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
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::other("worker stdin pipe missing")
        })?;
        let mut stdout = child.stdout.take().ok_or_else(|| {
            io::Error::other("worker stdout pipe missing")
        })?;
        let hello_bytes = read_frame_async(&mut stdout).await?;
        let hello: Hello = msgpack::from_slice(&hello_bytes).map_err(|e| {
            io::Error::other(format!("decode Hello: {e}"))
        })?;
        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(io::Error::other(format!(
                "worker protocol_version {} does not match host {}",
                hello.protocol_version, PROTOCOL_VERSION,
            )));
        }
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: AtomicU64::new(1),
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
        let request_bytes = msgpack::to_vec_named(&request).map_err(|e| {
            io::Error::other(format!("encode RunRequest: {e}"))
        })?;
        write_frame_async(&mut self.stdin, &request_bytes).await?;
        let response_bytes = read_frame_async(&mut self.stdout).await?;
        let response: RunResponse = msgpack::from_slice(&response_bytes).map_err(|e| {
            io::Error::other(format!("decode RunResponse: {e}"))
        })?;
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
/// `NU_SH_MCP_WORKER_PATH` env var when set; otherwise resolves to a
/// sibling `nu_sh_mcp_worker` alongside the current executable.
///
/// Why: the integration test suite uses `CARGO_BIN_EXE_*` env var to
/// point each test at a deterministic worker binary path
/// (test-specific, not the installed one); production runs use the
/// sibling-of-current-exe path that `cargo install` produces.
///
/// Where: called by `WorkerHandle::spawn` to find the binary to
/// launch. Not exposed to other modules.
fn resolve_worker_bin() -> io::Result<PathBuf> {
    if let Ok(p) = std::env::var("NU_SH_MCP_WORKER_PATH") {
        return Ok(PathBuf::from(p));
    }
    let current = std::env::current_exe()?;
    let dir = current.parent().ok_or_else(|| {
        io::Error::other("current_exe has no parent directory")
    })?;
    Ok(dir.join("nu_sh_mcp_worker"))
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
