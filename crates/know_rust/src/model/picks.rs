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

impl PickSelector {
    /// What: `true` if this selector is `Carried` (the walker surfaced
    /// it as transitive context), `false` for `Picked`.
    ///
    /// Why: predicate form reads more cleanly than `matches!(s,
    /// PickSelector::Carried)` at use sites.
    ///
    /// Where: walker + picker code that branches on carry-vs-picked.
    pub const fn carried(&self) -> bool {
        matches!(self, Self::Carried)
    }

    /// What: `true` if this selector is `Picked`, the inverse of
    /// [`Self::carried`].
    pub const fn picked(&self) -> bool {
        matches!(self, Self::Picked)
    }

    /// What: snake_case wire form (`picked` / `carried`). Matches the
    /// serde-derived JSON representation.
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::Picked => "picked",
            Self::Carried => "carried",
        }
    }
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

    /// What: snake_case wire form for the group (e.g.
    /// `trait_functions`, `implementation_functions`,
    /// `structure`). Matches the serde-derived JSON representation
    /// and the `<group>:<name>` pattern key shape from the
    /// picks-data refactor plan.
    ///
    /// Why: callers writing pattern keys ("group:name") need the
    /// wire form as a `&str` without round-tripping through serde.
    /// Const-evaluable so it can be used at compile time for static
    /// pattern lookups.
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::Traits => "traits",
            Self::TraitFunctions => "trait_functions",
            Self::Structure => "structure",
            Self::ImplementationFunctions => "implementation_functions",
            Self::Derives => "derives",
            Self::Utilities => "utilities",
            Self::Globals => "globals",
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

impl PickSet {
    /// What: the set-category scope this set belongs to.
    /// `Architecture`, `Public`, `InterCrate`, `Clique` are
    /// `WorkspaceWide`; `IntraCrate`, `InnerCrate` are `PerCrate`.
    ///
    /// Why: encodes the WorkspaceWide vs PerCrate partition on the
    /// type rather than in docs. The calibration matrix at refactor
    /// phase R4 indexes per `(PickGroup, PickSet)` cell but several
    /// weights apply at the set-category level; this lets callers
    /// route to the right calibration axis without re-deriving the
    /// partition.
    ///
    /// Where: planned use is `src/emit/picker.rs`'s
    /// `compute_significance_sets` per-set dispatch + the calibration
    /// matrix lookup. Mirrors the const-fn shape of
    /// [`PickGroup::carryable`].
    pub const fn category(&self) -> PickSetCategory {
        match self {
            Self::Architecture
            | Self::Public
            | Self::InterCrate
            | Self::Clique => PickSetCategory::WorkspaceWide,
            Self::IntraCrate | Self::InnerCrate => PickSetCategory::PerCrate,
        }
    }

    /// What: snake_case wire form (e.g. `inter_crate`,
    /// `intra_crate`). Matches the serde-derived JSON representation
    /// and the current `SignificanceSets` field names in
    /// `src/emit/picker.rs`.
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Public => "public",
            Self::InterCrate => "inter_crate",
            Self::Clique => "clique",
            Self::IntraCrate => "intra_crate",
            Self::InnerCrate => "inner_crate",
        }
    }
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

impl PickSetCategory {
    /// What: snake_case wire form (`workspace_wide` / `per_crate`).
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::WorkspaceWide => "workspace_wide",
            Self::PerCrate => "per_crate",
        }
    }
}

/// What: const array of the five carry-having pick groups.
///
/// Why: lets iteration + set-membership work in const contexts. The
/// length contract (5) plus [`NO_CARRY_GROUPS`]'s length (2) is
/// asserted at compile time to equal the total variant count (7);
/// adding a new `PickGroup` variant without categorizing it
/// fails the compile-time check.
pub const CARRY_GROUPS: [PickGroup; 5] = [
    PickGroup::Traits,
    PickGroup::TraitFunctions,
    PickGroup::Structure,
    PickGroup::ImplementationFunctions,
    PickGroup::Derives,
];

/// What: const array of the two no-carry pick groups (`Utilities`,
/// `Globals`).
pub const NO_CARRY_GROUPS: [PickGroup; 2] = [
    PickGroup::Utilities,
    PickGroup::Globals,
];

/// What: const array of the four workspace-wide significance sets.
pub const WORKSPACE_WIDE_SETS: [PickSet; 4] = [
    PickSet::Architecture,
    PickSet::Public,
    PickSet::InterCrate,
    PickSet::Clique,
];

/// What: const array of the two per-crate significance sets.
pub const PER_CRATE_SETS: [PickSet; 2] = [
    PickSet::IntraCrate,
    PickSet::InnerCrate,
];

const _CARRY_GROUPS_EXHAUSTIVE: () = {
    if CARRY_GROUPS.len() + NO_CARRY_GROUPS.len() != 7 {
        panic!(
            "CARRY_GROUPS + NO_CARRY_GROUPS must cover all 7 PickGroup variants; \
             add the new variant to one of the const arrays"
        );
    }
};

const _PICK_SETS_EXHAUSTIVE: () = {
    if WORKSPACE_WIDE_SETS.len() + PER_CRATE_SETS.len() != 6 {
        panic!(
            "WORKSPACE_WIDE_SETS + PER_CRATE_SETS must cover all 6 PickSet variants; \
             add the new variant to one of the const arrays"
        );
    }
};

/// What: a single picks-data pattern - the `(group, name)` pair that
/// the refactored picker uses as a `pattern_metrics` key. Replaces
/// the current stringly-typed `kind:name` key form.
///
/// Why: typing the key shape lets walker + picker + emit code be
/// typed end-to-end against `Pattern` rather than passing strings
/// that have to be parsed at each consumer. The `Display` impl
/// renders as `<group_wire>:<name>` matching the picks-data-model.md
/// wire shape, so backward-compatible JSON keys are still produced
/// during the refactor.
///
/// Where: planned consumers are the items walker (one `Pattern`
/// emitted per pick), the picker's `SignificanceSets` (indexed by
/// `Pattern`), and the emit composers (orientation.md + reference.md
/// render `Pattern` via its `Display` impl).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Pattern {
    pub group: PickGroup,
    pub name: String,
}

impl Pattern {
    /// What: constructor accepting any string-convertible name.
    pub fn new(group: PickGroup, name: impl Into<String>) -> Self {
        Self {
            group,
            name: name.into(),
        }
    }
}

impl std::fmt::Display for Pattern {
    /// What: renders as `<group_wire>:<name>` (e.g.
    /// `structure:Component`, `derives:Clone`,
    /// `implementation_functions:render`). Matches the
    /// picks-data-model.md wire shape.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.group.wire(), self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickgroup_carryable_partition() {
        let carry = CARRY_GROUPS.iter().filter(|g| g.carryable()).count();
        let no_carry = NO_CARRY_GROUPS.iter().filter(|g| !g.carryable()).count();
        assert_eq!(carry, 5);
        assert_eq!(no_carry, 2);
    }

    #[test]
    fn pickset_category_partition() {
        let ww = WORKSPACE_WIDE_SETS
            .iter()
            .filter(|s| s.category() == PickSetCategory::WorkspaceWide)
            .count();
        let pc = PER_CRATE_SETS
            .iter()
            .filter(|s| s.category() == PickSetCategory::PerCrate)
            .count();
        assert_eq!(ww, 4);
        assert_eq!(pc, 2);
    }

    #[test]
    fn pickselector_round_trip() {
        for sel in [PickSelector::Picked, PickSelector::Carried] {
            let json = serde_json::to_string(&sel).expect("serialize");
            let parsed: PickSelector = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(sel, parsed);
            assert_eq!(json.trim_matches('"'), sel.wire());
        }
    }

    #[test]
    fn pickgroup_round_trip() {
        for g in CARRY_GROUPS.iter().chain(NO_CARRY_GROUPS.iter()) {
            let json = serde_json::to_string(g).expect("serialize");
            let parsed: PickGroup = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*g, parsed);
            assert_eq!(json.trim_matches('"'), g.wire());
        }
    }

    #[test]
    fn pickset_round_trip() {
        for s in WORKSPACE_WIDE_SETS.iter().chain(PER_CRATE_SETS.iter()) {
            let json = serde_json::to_string(s).expect("serialize");
            let parsed: PickSet = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*s, parsed);
            assert_eq!(json.trim_matches('"'), s.wire());
        }
    }

    #[test]
    fn picksetcategory_round_trip() {
        for c in [PickSetCategory::WorkspaceWide, PickSetCategory::PerCrate] {
            let json = serde_json::to_string(&c).expect("serialize");
            let parsed: PickSetCategory = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(c, parsed);
            assert_eq!(json.trim_matches('"'), c.wire());
        }
    }

    #[test]
    fn pattern_display() {
        let p = Pattern::new(PickGroup::Structure, "Component");
        assert_eq!(p.to_string(), "structure:Component");
        let p2 = Pattern::new(PickGroup::ImplementationFunctions, "render");
        assert_eq!(p2.to_string(), "implementation_functions:render");
        let p3 = Pattern::new(PickGroup::TraitFunctions, "poll");
        assert_eq!(p3.to_string(), "trait_functions:poll");
    }

    #[test]
    fn pattern_hashable() {
        use std::collections::HashMap;
        let mut map: HashMap<Pattern, usize> = HashMap::new();
        map.insert(Pattern::new(PickGroup::Structure, "Component"), 1);
        map.insert(Pattern::new(PickGroup::Derives, "Clone"), 2);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&Pattern::new(PickGroup::Structure, "Component")),
            Some(&1)
        );
    }
}
