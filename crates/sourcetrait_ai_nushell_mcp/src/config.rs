use crate::*;

/// What: the host's runtime configuration -- the state-store coordinate
/// (`id` + `namespace`), the agent work dir, and the operator-denied tool
/// set, parsed from the host CLI (`--id` / `--namespace` / `--workdir` /
/// `--deny`) by `cli::host_main`.
///
/// Why: one binary serves every variant; the operator's `.mcp.json` entry
/// (or CLI invocation) selects the store and the tool surface at runtime.
/// This replaces the retired compile-artifact split (`BuildTarget`
/// Main/Test, the `_test` bin pair, and the `_test`-suffix library-name
/// gate -- per-store isolation supersedes it). The values are TRUSTED
/// OPERATOR CONFIG: `id` / `namespace` are deliberately not
/// ident-validated at this boundary; a bad value surfaces as the natural
/// downstream error (improper configuration, the operator's domain).
///
/// Where: stored in `CONFIG` by `cli::host_main` before `run_server` /
/// `run_oneshot`; read via `config()` by the path helpers
/// (`cache::data_base_dir` / `cache::cache_base_dir`), the router
/// assembly (`NuSh::tool_router`), the worker spawn env
/// (`WorkerHandle::spawn`), and the info/serverInfo surfaces.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub id: String,
    pub namespace: String,
    /// The agent's working directory (its repo/work root), resolved by
    /// `cli::resolve_work_dir` (an explicit `--workdir` tilde-expanded;
    /// absent -> `<home>/proj/equip/<id>`). Trusted operator config -- no
    /// existence check. Exported to every worker as EQUIP_WORK_DIR and
    /// reported by info(); the host itself never reads it.
    pub work_dir: PathBuf,
    pub deny: DenySet,
}

/// What: the tools an operator may deny via `--deny` -- the eval surfaces
/// (run / rerun / interact / call), the skill writer (learn), and the
/// authoring set (new / commit / library). The core four (info, inspect,
/// processes, kill) are not deniable and have no variant here.
///
/// Why: deny is enforced at ROUTER ASSEMBLY (`NuSh::tool_router` adds a
/// denied tool's router conditionally), so a denied tool is absent from
/// tools/list entirely -- no schema tokens in the agent's context, no
/// denied-error loop; a call anyway fails at the rmcp layer as an unknown
/// tool. There is NO run=>rerun implication: trusted operator config
/// lists exactly what it means (deny both when both are meant).
///
/// Where: parsed by `cli::parse_deniable` (clap value_parser, fail-fast
/// on unknown names); queried through `DenySet::denies`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeniableTool {
    Run,
    Rerun,
    Interact,
    Call,
    Learn,
    New,
    Commit,
    Library,
}

impl DeniableTool {
    /// Parse a `--deny` token (the tool's wire name); None for an unknown
    /// or non-deniable name (the caller rejects with the valid-token
    /// list).
    pub(crate) fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "run" => Self::Run,
            "rerun" => Self::Rerun,
            "interact" => Self::Interact,
            "call" => Self::Call,
            "learn" => Self::Learn,
            "new" => Self::New,
            "commit" => Self::Commit,
            "library" => Self::Library,
            _ => return None,
        })
    }
}

/// What: the set of operator-denied tools, as parsed from `--deny`'s csv.
/// Empty by default (nothing denied).
///
/// Why: a plain membership wrapper keeps the router-assembly call sites
/// readable (`deny.denies(DeniableTool::Run)`); the set is tiny and
/// read-only after startup.
///
/// Where: carried on `Config`; queried by `NuSh::tool_router`.
#[derive(Debug, Clone, Default)]
pub(crate) struct DenySet {
    denied: Vec<DeniableTool>,
}

impl DenySet {
    pub(crate) fn new(denied: Vec<DeniableTool>) -> Self {
        Self { denied }
    }

    pub(crate) fn denies(&self, tool: DeniableTool) -> bool {
        self.denied.contains(&tool)
    }
}

/// What: process-global storage for the active `Config`. Set once by
/// `cli::host_main` (serve and one-shot cli paths alike) before any code
/// path that reads.
///
/// Why: the same shape the retired `BUILD_TARGET` OnceLock had -- the
/// binary entry point resolves the runtime variant once, the lib reads it
/// without threading a parameter through every path helper. Worker
/// processes never read it (the host passes everything they need across
/// the spawn boundary: log_dir per request, the libraries root + the
/// id/namespace pair as spawn env).
///
/// Where: written by `cli::host_main`; read through `config()`.
pub(crate) static CONFIG: OnceLock<Config> = OnceLock::new();

/// What: returns the active `Config`. Panics if called before
/// `cli::host_main` stored it (a startup-order bug -- CONFIG is set
/// before serve/one-shot dispatch, ahead of any reader).
///
/// Why: one accessor centralizes the "set at startup, panic otherwise"
/// invariant and keeps read sites terse.
///
/// Where: called by `cache::cache_base_dir`, `cache::data_base_dir`,
/// `NuSh::tool_router`, `WorkerHandle::spawn`, `tool::NuSh::info`, and
/// `tool::ServerHandler::get_info`.
pub(crate) fn config() -> &'static Config {
    CONFIG.get().expect("CONFIG set at startup")
}
