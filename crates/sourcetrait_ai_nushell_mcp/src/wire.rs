use crate::*;

pub(crate) const PROTOCOL_VERSION: u32 = 1;

/// What: the very first message a worker writes to its stdout after
/// spawn -- carries `protocol_version` so the host can fail-fast on
/// version skew before sending any RunRequests.
///
/// Why: workers and the host are separate binaries produced from the
/// same source tree; nothing guarantees they were built from the same
/// version. The Hello handshake makes a mismatch surface at spawn
/// time with a clear error instead of mid-call as a corrupted
/// msgpack frame.
///
/// Where: written by `worker::run::run_worker` immediately after
/// process start; read by `server::worker_handle::WorkerHandle::spawn`
/// which compares `protocol_version` against the host's
/// `PROTOCOL_VERSION` const.
#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct Hello {
    pub protocol_version: u32,
}

/// What: per-call message sent from host to worker over the
/// length-prefixed msgpack IPC channel. Carries a host-assigned
/// sequence id, the cache log dir, and the nushell source to
/// evaluate.
///
/// Why: pairing requests with sequence ids lets the worker_handle
/// detect channel desyncs (RunResponse.id must match the in-flight
/// RunRequest.id); pre-creating log_dir on the host side keeps the
/// worker from needing to know the cache layout; sending source as
/// a single String matches nushell's compile-from-text model.
///
/// Where: serialized by `WorkerHandle::send_request`, written via
/// `ipc::framing::write_frame_async`; read on the worker side by
/// `worker::request_loop::serve` which decodes one per IPC frame.
#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct RunRequest {
    pub id: u64,
    /// Per-call cache dir already created by the host. Worker opens
    /// `<log_dir>/stdout` and `<log_dir>/stderr` and redirects the
    /// engine's external stdout/stderr to those files instead of the
    /// worker's fd 1/2 (which the IPC framing in `serve` owns).
    pub log_dir: PathBuf,
    pub source: String,
}

/// What: per-call reply from worker to host. Echoes the request id,
/// reports ok/fail, and either carries the msgpack-encoded result
/// `value` (on success) or a human-readable `error` string.
///
/// Why: a single response variant covers both branches so the IPC
/// framing layer doesn't need to know about typed alternatives;
/// `ok=false` paired with `value=[]` + `error=Some(...)` is the
/// failure shape, which the host's `dispatch_to_worker` translates
/// into an rmcp `ErrorData`.
///
/// Where: built by `worker::request_loop::serve` (success +
/// caught-panic + caught-eval-error variants); written via
/// `ipc::framing::write_frame`; read by `WorkerHandle::send_request`
/// which validates the id, checks ok, and either returns the bytes
/// or surfaces the error.
#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct RunResponse {
    pub id: u64,
    pub ok: bool,
    pub value: Vec<u8>,
    pub error: Option<String>,
}
