use crate::*;

/// Categories under `<cache_base_dir>/` that partition log dirs by the
/// invocation kind that produced them.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CacheKind {
    Runs,
    Interacts,
    /// `Calls` is reserved for the `call()` tool that lands in post-MTP
    /// slice 3; pre-defined here so the partitioning is fixed at the type
    /// level rather than added retroactively.
    #[allow(dead_code)]
    Calls,
}

impl CacheKind {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Runs => "runs",
            Self::Interacts => "interacts",
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
        .join(lib_empower::consts::NU_SH_MCP)
}

pub(crate) fn cache_kind_dir(kind: CacheKind) -> PathBuf {
    cache_base_dir().join(kind.dir_name())
}

pub(crate) fn cache_dir(
    kind: CacheKind,
    nonce: lib_empower::Nonce,
) -> PathBuf {
    cache_kind_dir(kind).join(nonce.to_string())
}
