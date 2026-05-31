use crate::*;

pub(crate) const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct Hello {
    pub protocol_version: u32,
}

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

#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct RunResponse {
    pub id: u64,
    pub ok: bool,
    pub value: Vec<u8>,
    pub error: Option<String>,
}
