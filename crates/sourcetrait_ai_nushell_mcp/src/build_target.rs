use crate::*;

/// What: which build of nushell_mcp this binary represents -- production
/// (`Main`) or `_test` sandbox (`Test`). The lib code branches on the
/// active value to namespace XDG cache/data dirs, set the reported
/// `serverInfo.name`, and enforce the `_test`-suffix gate on library
/// registration.
///
/// Why: the `_test` variant shares the production codebase but runs
/// against isolated state (separate XDG dirs, separate library repo)
/// so a catastrophic bug in the `_test` surface can't damage the
/// production MCP the agent relies on for normal operation. Encoding
/// the target as a runtime enum with a single compilation keeps the
/// source DRY across the two variants.
///
/// Where: each binary entry point hardcodes its target. `src/main.rs`
/// calls `run_server(BuildTarget::Main)`; `src/bin/nushell_mcp_test.rs`
/// calls `run_server(BuildTarget::Test)`. `BUILD_TARGET` lives only in
/// host processes -- worker processes never call `build_target()`
/// because they receive `log_dir` from the host per request and don't
/// resolve XDG cache/data themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    Main,
    Test,
}

impl BuildTarget {
    /// What: returns the build target's canonical name string --
    /// `"nushell_mcp"` for `Main`, `"nushell_mcp_test"` for `Test`. Both
    /// strings come from `lib_empower::consts` so the two names live
    /// in a single sister crate, not duplicated here.
    ///
    /// Why: every host-side path helper + the reported `serverInfo`
    /// name + the `info()` envelope all derive their string from this
    /// accessor, so renaming the target only touches the sister-crate
    /// const.
    ///
    /// Where: called by `cache::cache_base_dir`, `cache::data_base_dir`,
    /// `tool::NuSh::info`, and `tool::ServerHandler::get_info`. Worker
    /// processes never call this -- see the type docstring.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Main => lib_empower::consts::NUSHELL_MCP,
            Self::Test => lib_empower::consts::NUSHELL_MCP_TEST,
        }
    }

    /// What: returns `true` iff this is the `Test` variant. Convenience
    /// over a match on the enum for the single gate that branches on
    /// target-equality.
    ///
    /// Why: the `_test`-suffix library-name validator in
    /// `library::register_library_impl` / `import_library_impl` reads
    /// "are we on the test variant?" not "what target string is this?";
    /// expressing the gate as `build_target().is_test()` keeps the
    /// readable intent at the call site.
    ///
    /// Where: called by `library::register_library_impl` and
    /// `library::import_library_impl`.
    pub(crate) fn is_test(self) -> bool {
        matches!(self, Self::Test)
    }
}

/// What: process-global storage for the active `BuildTarget`. Set once
/// at startup by `run_server(target)` via `BUILD_TARGET.set(target)`;
/// thereafter every lib reader fetches the value through
/// `build_target()`.
///
/// Why: the two binary entry points (`nushell_mcp` + `nushell_mcp_test`)
/// share one lib compilation; the lib can't have a compile-time const
/// that differs per binary, so the entry point sets a runtime value
/// that the lib reads. `OnceLock` is the matching primitive --
/// set-once + read-many-without-locking. `Sync` lets every thread
/// (rmcp dispatch tasks, worker spawns, the reaper) read race-free
/// after the single startup write.
///
/// Where: written by `server::run::run_server`; read indirectly through
/// `build_target()`.
pub(crate) static BUILD_TARGET: OnceLock<BuildTarget> = OnceLock::new();

/// What: returns the active `BuildTarget`. Panics if called before
/// `run_server(target)` ran (which would be a bug -- the startup order
/// is fixed: `run_server` sets `BUILD_TARGET` as its first action,
/// before any code path that reads).
///
/// Why: every reader gets a single accessor instead of touching the
/// `OnceLock` directly, so the "set at startup, panic otherwise"
/// invariant is centralized + the read sites stay terse.
///
/// Where: called by `cache::cache_base_dir`, `cache::data_base_dir`,
/// `library::register_library_impl`, `library::import_library_impl`,
/// `tool::NuSh::info`, `tool::ServerHandler::get_info`. Worker
/// processes never call this.
pub(crate) fn build_target() -> BuildTarget {
    *BUILD_TARGET.get().expect("BUILD_TARGET set at startup")
}
