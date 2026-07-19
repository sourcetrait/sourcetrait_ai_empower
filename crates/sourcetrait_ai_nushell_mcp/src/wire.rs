use crate::*;

pub(crate) const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct Hello {
    pub protocol_version: u32,
}

#[derive(Debug, ser::Serialize, ser::Deserialize)]
pub(crate) struct RunRequest {
    pub id: u64,
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
