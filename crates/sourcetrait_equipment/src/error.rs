use crate::*;

pub type EquipmentResult<T> = Result<T, EquipmentError>;

#[derive(Debug, snafu::Snafu)]
pub enum EquipmentError {
    File {
        source: io::Error,
        path: PathBuf,
        op: IoErr,
    },
    TomlFileRead {
        source: toml::de::Error,
        path: PathBuf,
    },
    SemVer {
        source: semver::Error,
        ver: String,
    },
    Spdx {
        source: spdx::ParseError,
        spdx: String,
    },
    String {
        str: String,
        err: StrErr,
    },
    Email {
        address: String,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IoErr {
    Read,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum StrErr {
    Case(StrCase),
    MaxLen(usize),
    MinLen(usize),
}
