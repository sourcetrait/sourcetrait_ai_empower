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
    /// `call()` per-invocation logs at `calls/<nonce>/{stdout,stderr}`.
    Calls,
}

impl CacheKind {
    /// What: returns the static directory name string (`"runs"`,
    /// `"interacts"`, `"closures"`, `"calls"`) for each variant.
    ///
    /// Why: keeps the variant-to-string mapping in one place so the
    /// path helpers stay typed (CacheKind) while the disk layout
    /// stays stringly-keyed; renaming a dir means changing one match
    /// arm.
    ///
    /// Where: called internally by `cache_kind_dir` to compose the
    /// per-kind subdir; never exposed externally.
    fn dir_name(self) -> &'static str {
        match self {
            Self::Runs => "runs",
            Self::Interacts => "interacts",
            Self::Closure => "closures",
            Self::Calls => "calls",
        }
    }
}

/// What: lazily-initialized handle to the `directories` crate's
/// `BaseDirs` view of the platform's XDG paths (cache, data, config,
/// etc.). Initialized on first access via `LazyLock`; panics if
/// `BaseDirs::new()` returns None (which the docs document as
/// "platform with no home dir" -- unsupported here).
///
/// Why: `BaseDirs` honors `$XDG_CACHE_HOME` and `$XDG_DATA_HOME` env
/// vars, which the test suite uses to isolate per-test cache/data
/// directories. Sharing a single static keeps all cache + data paths
/// consistent with the same XDG view.
///
/// Where: used by `cache_base_dir` and `data_base_dir` (the only two
/// XDG-rooted path helpers in this module). Indirectly used by every
/// caller of those helpers, which is essentially the whole server
/// side.
pub(crate) static BASE_DIRS: LazyLock<dirs::BaseDirs> =
    LazyLock::new(|| dirs::BaseDirs::new().expect("BaseDirs::new failed"));

/// What: returns
/// `$XDG_CACHE_HOME/sourcetrait/nushell_mcp/<id>/<namespace>/`, the
/// per-store cache root where per-call stdout/stderr logs
/// (runs/interacts/calls) and the content-addressed closure cache
/// (closures) live. The vendor segment is
/// `lib_empower::consts::SOURCETRAIT`; the app segment is
/// `consts::NUSHELL_MCP`; the `<id>/<namespace>` leaf is the runtime
/// store coordinate from `config()`.
///
/// Why: XDG cache is the platform-correct location for regenerable
/// artifacts; the `sourcetrait/` vendor segment namespaces every
/// sourcetrait app under one parent, and the id/namespace leaf keeps
/// every store (each agent, each test namespace) fully isolated so
/// co-running hosts never touch the same files.
///
/// Where: called by `cache_kind_dir` (which appends a CacheKind
/// subdir) and `closure_cache_file` (which appends `closures/` +
/// rerun_id + `.json`).
pub(crate) fn cache_base_dir() -> PathBuf {
    BASE_DIRS
        .cache_dir()
        .join(lib_empower::consts::SOURCETRAIT)
        .join(lib_empower::consts::NUSHELL_MCP)
        .join(&config().id)
        .join(&config().namespace)
}

/// What: returns
/// `$XDG_DATA_HOME/sourcetrait/nushell_mcp/<id>/<namespace>/`, the
/// per-store data root where the signing keypair and the libraries git
/// repo live. The vendor segment is `lib_empower::consts::SOURCETRAIT`;
/// the app segment is `consts::NUSHELL_MCP`; the `<id>/<namespace>`
/// leaf is the runtime store coordinate from `config()`.
///
/// Why: XDG data is the platform-correct location for non-regenerable
/// content; losing it would mean losing library registrations and
/// the signing keypair. The id-first leaf order groups an agent's
/// whole state (every namespace) under one subtree, so wiping or
/// backing an agent up is one directory operation.
///
/// Where: called by `server::library` path helpers (`keypair_dir`,
/// `libraries_dir`, `library_dir`, `library_meta_path`,
/// `library_docs_dir`) to compose every substrate path.
pub(crate) fn data_base_dir() -> PathBuf {
    BASE_DIRS
        .data_dir()
        .join(lib_empower::consts::SOURCETRAIT)
        .join(lib_empower::consts::NUSHELL_MCP)
        .join(&config().id)
        .join(&config().namespace)
}

/// What: returns the `<cache_base>/<kind>/` directory for the given
/// `CacheKind`, e.g. `$XDG_CACHE_HOME/sourcetrait/nushell_mcp/runs/`.
///
/// Why: per-kind subdir partitioning keeps run/interact/call logs
/// from colliding on the per-call nonce and gives the agent a
/// predictable place to look for each tool's artifacts.
///
/// Where: called by `cache_dir` and `closure_cache_file` as the
/// shared prefix; never returned directly to callers.
pub(crate) fn cache_kind_dir(kind: CacheKind) -> PathBuf {
    cache_base_dir().join(kind.dir_name())
}

/// What: returns the per-call log dir for a specific `(CacheKind,
/// Nonce)` pair, e.g.
/// `$XDG_CACHE_HOME/sourcetrait/nushell_mcp/runs/<nonce>/`. The
/// dir is NOT created by this function; callers handle
/// `fs::create_dir_all`.
///
/// Why: each tool call (run/interact/call) gets its own nonce-named
/// subdir to hold stdout + stderr files redirected from the engine's
/// external commands. Sharing a parent dir per kind keeps the
/// filesystem layout legible.
///
/// Where: called by `server::tool::dispatch_to_worker` (creates the
/// dir, includes it in the RunRequest's `log_dir` field). The worker
/// opens `<log_dir>/stdout` and `<log_dir>/stderr` for engine
/// redirect.
pub(crate) fn cache_dir(kind: CacheKind, nonce: Nonce) -> PathBuf {
    cache_kind_dir(kind).join(nonce.to_string())
}

/// Path of a content-addressed `closures/<rerun_id>.json` file.
/// Diverges from `cache_dir()` which returns a per-call subdir for
/// `runs/` and `interacts/`.
pub(crate) fn closure_cache_file(rerun_id: &str) -> PathBuf {
    cache_kind_dir(CacheKind::Closure).join(format!("{rerun_id}.json"))
}
