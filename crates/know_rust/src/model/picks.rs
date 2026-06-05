#![allow(dead_code)]
#[allow(unused_imports)]
use crate::*;

/// What: which selector classified a pattern into the picks data.
/// `Picked` means the picker chose this pattern from the workspace's
/// significance signals; `Carried` means the walker surfaced this
/// pattern as one-hop transitive context for a `Picked` entry.
///
/// Why: the rust_know data model (`notes/know_rust/picks-data-model.md`)
/// distinguishes the primary architectural protagonists (Picked) from
/// the contextual deps the reader needs to understand them (Carried).
/// One-hop carry is bounded by the carry depth and the picked-item
/// cap; no separate carry cap (the_user 2026-06-05).
///
/// Where: typed reference for the picks-data refactor at
/// `notes/know_rust/tasks/picks-data-model-refactor.md`. Threaded
/// through `pattern_metrics` and the picker's enriched output once
/// the refactor lands; before then, this is documentation in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickSelector {
    Picked,
    Carried,
}

/// What: the seven semantic groups partitioning the item kinds the
/// picker considers. Carry-having groups (`Traits`, `TraitFunctions`,
/// `Structure`, `ImplementationFunctions`, `Derives`) surface
/// transitive one-hop reader-context items via the picker's `Carried`
/// selector; no-carry groups (`Utilities`, `Globals`) do not.
///
/// Why: replaces the current picker's syntactic `kind:name` keys
/// (`trait_impl` / `derive` / `type_usage` / `pub_type` / etc.) with a
/// semantic taxonomy aligned with how the reader categorizes items.
/// The `Structure` vs `Traits` split disambiguates the current
/// `pub_type` conflation.
///
/// Where: typed reference for the picks-data refactor. The
/// `pattern_metrics` key shape will become `<group>:<name>` once the
/// refactor's R1 + R3 phases land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickGroup {
    Traits,
    TraitFunctions,
    Structure,
    ImplementationFunctions,
    Derives,
    Utilities,
    Globals,
}

impl PickGroup {
    /// What: `true` if items of this group carry one-hop transitive
    /// context (the carry-having groups: `Traits` / `TraitFunctions`
    /// / `Structure` / `ImplementationFunctions` / `Derives`),
    /// `false` if the group is "no carry" (`Utilities` / `Globals`).
    ///
    /// Why: encodes the picks-data model's carry partition (per
    /// `notes/know_rust/picks-data-model.md` "Pick groups" section)
    /// directly on the type. Lets walker logic at refactor phase R2
    /// decide whether to surface one-hop carry facts during the
    /// syn-based scan, and lets the picker validate that a `Carried`
    /// selector only attaches to a carryable group.
    ///
    /// Where: planned consumers are
    /// `crates/know_rust/src/scan/items/` walker logic and the
    /// picker integration in `src/emit/picker.rs`. The `const`
    /// shape lets it be used in const contexts (e.g. compile-time
    /// validation of carry-group sets) once those consumers land.
    pub const fn carryable(&self) -> bool {
        match self {
            Self::Traits
            | Self::TraitFunctions
            | Self::Structure
            | Self::ImplementationFunctions
            | Self::Derives => true,
            Self::Utilities | Self::Globals => false,
        }
    }
}

/// What: the six significance sets the picker partitions patterns
/// into. Workspace-wide sets (`Architecture`, `Public`, `InterCrate`,
/// `Clique`) cover cross-crate signals; per-crate sets (`IntraCrate`,
/// `InnerCrate`) cover within-crate signals.
///
/// Why: matches the phase-5a-shipped picker's significance set
/// structure exactly. The refactor preserves the set membership;
/// only the per-pattern keys move from `kind:name` to `group:name`.
///
/// Where: typed reference for the picks-data refactor. Will replace
/// the implicit string keys currently used in `SignificanceSets`
/// (`crates/know_rust/src/emit/picker.rs`) when the refactor lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickSet {
    Architecture,
    Public,
    InterCrate,
    Clique,
    IntraCrate,
    InnerCrate,
}

/// What: the two scope categories the six `PickSet` variants
/// partition into. `WorkspaceWide` covers `Architecture` + `Public` +
/// `InterCrate` + `Clique`; `PerCrate` covers `IntraCrate` +
/// `InnerCrate`.
///
/// Why: the cap calibration matrix at refactor phase R4 indexes per
/// `(PickGroup, PickSet)` cell, but several calibration weights apply
/// at the set-category level instead (e.g. workspace-wide picks
/// share an SLOC-scaled base; per-crate picks scale per crate). This
/// enum is the calibration scope axis.
///
/// Where: typed reference for the picks-data refactor's calibration
/// matrix. The current `calibration.toml` does not yet encode a
/// per-set-category dimension; refactor phase R4 adds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickSetCategory {
    WorkspaceWide,
    PerCrate,
}
