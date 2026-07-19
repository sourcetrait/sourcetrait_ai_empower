use crate::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CacheKind {
    Runs,
    Interacts,
    Closure,
    Calls,
}

impl CacheKind {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Runs => "runs",
            Self::Interacts => "interacts",
            Self::Closure => "closures",
            Self::Calls => "calls",
        }
    }
}

pub(crate) static BASE_DIRS: LazyLock<dirs::BaseDirs> =
    LazyLock::new(|| dirs::BaseDirs::new().expect("BaseDirs::new failed"));

pub(crate) fn cache_base_dir() -> PathBuf {
    BASE_DIRS
        .cache_dir()
        .join(lib_empower::consts::SOURCETRAIT)
        .join(lib_empower::consts::NUSHELL_MCP)
        .join(&config().id)
        .join(&config().namespace)
}

pub(crate) fn data_base_dir() -> PathBuf {
    BASE_DIRS
        .data_dir()
        .join(lib_empower::consts::SOURCETRAIT)
        .join(lib_empower::consts::NUSHELL_MCP)
        .join(&config().id)
        .join(&config().namespace)
}

pub(crate) fn cache_kind_dir(kind: CacheKind) -> PathBuf {
    cache_base_dir().join(kind.dir_name())
}

pub(crate) fn cache_dir(kind: CacheKind, nonce: Nonce) -> PathBuf {
    cache_kind_dir(kind).join(nonce.to_string())
}

pub(crate) fn closure_cache_file(rerun_id: &str) -> PathBuf {
    cache_kind_dir(CacheKind::Closure).join(format!("{rerun_id}.json"))
}
