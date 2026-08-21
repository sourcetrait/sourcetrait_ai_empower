use crate::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CacheKind {
    Runs,
    Interacts,
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
        .join(lib_grammar::consts::SOURCETRAIT)
        .join(lib_grammar::consts::GRAMMAR)
        .join(&config().id)
        .join(&config().namespace)
}

pub(crate) fn data_base_dir() -> PathBuf {
    BASE_DIRS
        .data_dir()
        .join(lib_grammar::consts::SOURCETRAIT)
        .join(lib_grammar::consts::GRAMMAR)
        .join(&config().id)
        .join(&config().namespace)
}

pub(crate) fn cache_kind_dir(kind: CacheKind) -> PathBuf {
    cache_base_dir().join(kind.dir_name())
}

pub(crate) fn cache_dir(kind: CacheKind, nonce: &datum::NoncePair) -> PathBuf {
    cache_kind_dir(kind).join(nonce.as_str())
}

pub(crate) const BODY_FILE: &str = "body.nuon";

/// The cached run body, co-located at `runs/<nonce>/body.nuon`.
pub(crate) fn run_body_file(nonce: &str) -> PathBuf {
    cache_kind_dir(CacheKind::Runs).join(nonce).join(BODY_FILE)
}
