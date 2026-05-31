use crate::*;

/// Categories under `<cache_base_dir>/` that partition log dirs by the
/// invocation kind that produced them.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CacheKind {
    Runs,
    Interacts,
    /// Content-addressed `closures/<rerun_id>.json` files written by
    /// `run()` so that `rerun()` can re-evaluate the same closure with
    /// new args without the agent re-sending the body.
    Closure,
    /// `Calls` is reserved for the `call()` tool (post-MTP slice 3);
    /// pre-defined here so the partitioning is fixed at the type level.
    #[allow(dead_code)]
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

/// Path of a content-addressed `closures/<rerun_id>.json` file.
/// Diverges from `cache_dir()` which returns a per-call subdir for
/// `runs/` and `interacts/`.
pub(crate) fn closure_cache_file(rerun_id: &str) -> PathBuf {
    cache_kind_dir(CacheKind::Closure).join(format!("{rerun_id}.json"))
}
