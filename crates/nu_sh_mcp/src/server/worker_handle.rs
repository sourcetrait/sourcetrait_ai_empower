use crate::*;

use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) struct WorkerHandle {
    #[allow(dead_code)]
    child: tk::Child,
    stdin: tk::ChildStdin,
    stdout: tk::ChildStdout,
    next_id: AtomicU64,
}

impl WorkerHandle {
    pub(crate) async fn spawn() -> io::Result<Self> {
        let worker_bin = resolve_worker_bin()?;
        let mut child = tk::Command::new(&worker_bin)
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
