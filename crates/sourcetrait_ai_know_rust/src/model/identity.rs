use crate::*;

/// What: provenance of one workspace unit - the host repo itself, a
/// vendored git submodule (with its .gitmodules URL and the gitlink
/// rev when readable), or plain in-repo vendored source.
///
/// Why: identity = provenance, names = bindings (the_user ruling,
/// 2026-06-10). A vendored fork shares its name (and most item
/// names) with the project it forked while being a DIFFERENT THING;
/// the model refuses to conflate by carrying the provenance, and it
/// never claims fork-OF lineage (world knowledge; agent-layer).
///
/// Where: held by `WorkspaceUnit`; converted to the fingerprint's
/// `workspace_units` wire shape in `characterize::run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnitProvenance {
    Host,
    Submodule { url: String, rev: Option<String> },
    InRepo,
}

impl UnitProvenance {
    /// What: the snake_case wire token for the provenance kind
    /// (`host` / `submodule` / `in_repo`).
    ///
    /// Why: the fingerprint serializes provenance as a small tagged
    /// record; the explicit token keeps the wire schema closed.
    ///
    /// Where: called when building `UnitProvenanceWire` entries in
    /// `characterize::run`.
    pub(crate) fn wire_kind(&self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Submodule { .. } => "submodule",
            Self::InRepo => "in_repo",
        }
    }
}

/// What: one workspace unit discovered in a repo - the host
/// workspace or an embedded (vendored) workspace reached through an
/// in-repo path-dep. `root_dir` is the unit's repo-relative
/// workspace root (`.` for the host) and doubles as the unit key;
/// `members` are the unit's package names; `populated` is false for
/// a declared-but-absent unit (empty submodule placeholder), which
/// still carries full identity via its provenance.
///
/// Why: an embedded workspace is never flattened into the host
/// membership ("workspace as a crate" is rejected modeling); units
/// keep per-crate attribution honest and give the orientation a
/// structural place to surface the non-conflation warning.
///
/// Where: produced by `characterize::cargo_toml::find_crates` inside
/// `WorkspaceDiscovery`; serialized into the fingerprint's
/// `workspace_units` map.
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceUnit {
    pub(crate) root_dir: String,
    pub(crate) provenance: UnitProvenance,
    pub(crate) members: Vec<String>,
    pub(crate) populated: bool,
}

/// What: the full result of workspace discovery - the per-package
/// crate map (host + populated unit members, each `CrateInfo` tagged
/// with its unit), the workspace roots, and the unit table.
///
/// Why: discovery now produces structure beyond a flat crate list;
/// returning one value keeps `find_crates`' contract explicit
/// instead of growing a tuple.
///
/// Where: returned by `find_crates`; consumed by
/// `characterize::run::characterize`.
#[derive(Debug, Default)]
pub(crate) struct WorkspaceDiscovery {
    pub(crate) crates: indexmap::IndexMap<String, CrateInfo>,
    pub(crate) workspace_roots: Vec<String>,
    pub(crate) units: indexmap::IndexMap<String, WorkspaceUnit>,
}
