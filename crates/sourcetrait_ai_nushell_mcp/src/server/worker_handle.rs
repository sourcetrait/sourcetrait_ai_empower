use crate::*;

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
    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    #[allow(dead_code)]
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }
}

pub(crate) fn kill_worker_pid(pid: u32) {
    let _ = sys::kill(sys::Pid::from_raw(pid as i32), sys::Signal::SIGKILL);
}

impl WorkerHandle {
    pub(crate) async fn spawn(mode: Mode) -> io::Result<Self> {
        let worker_bin = resolve_worker_bin()?;
        let mode_arg = match mode {
            Mode::Stateless => "stateless",
            Mode::Stateful => "stateful",
        };
        let mut child = tk::Command::new(&worker_bin)
            .arg("--mode")
            .arg(mode_arg)
            .env("NUSHELL_MCP_LIBRARIES_DIR", libraries_dir())
            .env("EQUIP_ID", &config().id)
            .env("EQUIP_NAMESPACE", &config().namespace)
            .env("EQUIP_WORK_DIR", &config().work_dir)
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
