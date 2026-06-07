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

/// What: the form sub-classification orthogonal to `PickGroup`, used
/// by the calibration's prose-budget matrix to route per-pick prose
/// budgets per (group, sub_form, set) cell. Five form discriminators
/// per the picks-data refactor's R4 phase: `Foundational` /
/// `Incidental` discriminate `Structure`; `Configured` / `Marker`
/// discriminate `Derives`; `Lifecycle` / `Marker` discriminate
/// `Traits`; `FreeFn` / `Macro` discriminate `Utilities`. Items in
/// `ImplementationFunctions`, `TraitFunctions`, `Globals` carry no
/// sub-form (the group already captures the relevant axis).
///
/// Why: the kp prose-budget matrix tuned 2026-06-05 (per
/// `notes/know_rust/knowledge_product_authoring.md`) lists 11 rows
/// across groups + sub-forms because some groups have a meaningful
/// budget discriminator (a configured derive earns much more prose
/// budget than a marker derive) and some don't. The `SubForm` enum
/// encodes that discriminator at the type level so the matrix lookup
/// is mechanical.
///
/// Where: populated by `crate::characterize::pattern_metrics`'s
/// classifier helpers from `facts.json` signals; persisted in
/// `PatternMetric::sub_form`; consumed by the calibration's
/// `ProseBudgetMatrix::budget_for` lookup at emit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubForm {
    Foundational,
    Incidental,
    Configured,
    Marker,
    Lifecycle,
    FreeFn,
    Macro,
}

impl SubForm {
    /// What: snake_case wire form matching the serde-derived JSON
    /// representation and the prose-budget matrix toml-section
    /// suffixes (e.g. `foundational` matches
    /// `[picker.prose_budget.structure_foundational]`).
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::Foundational => "foundational",
            Self::Incidental => "incidental",
            Self::Configured => "configured",
            Self::Marker => "marker",
            Self::Lifecycle => "lifecycle",
            Self::FreeFn => "free_fn",
            Self::Macro => "macro",
        }
    }

    /// What: parse a wire-form sub_form string back to a `SubForm`.
    /// Returns `None` on an unknown token.
    ///
    /// Why: pattern_metrics.json persists sub_form as its wire form;
    /// emit-time consumers read pattern_metrics as
    /// `serde_json::Value` (so the field is `Option<&str>`); this
    /// helper round-trips back to the typed enum for the matrix
    /// lookup.
    ///
    /// Where: called by
    /// `crate::emit::instance::sub_form_for_pattern` when reading
    /// each pattern's classified sub_form from the fingerprint.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "foundational" => Some(Self::Foundational),
            "incidental" => Some(Self::Incidental),
            "configured" => Some(Self::Configured),
            "marker" => Some(Self::Marker),
            "lifecycle" => Some(Self::Lifecycle),
            "free_fn" => Some(Self::FreeFn),
            "macro" => Some(Self::Macro),
            _ => None,
        }
    }

    /// What: `true` if this sub-form is meaningful for `group`,
    /// `false` if the (group, sub_form) pair is structurally invalid.
    /// `Foundational` / `Incidental` apply to `Structure`;
    /// `Configured` to `Derives`; `Marker` to both `Derives` and
    /// `Traits`; `Lifecycle` to `Traits`; `FreeFn` / `Macro` to
    /// `Utilities`. Other group + sub_form combinations are invalid.
    ///
    /// Why: the classifier should never emit a (group, sub_form) pair
    /// outside the validity table; the predicate lets debug
    /// assertions catch classifier bugs before they hit the matrix
    /// lookup.
    ///
    /// Where: planned use is debug assertions in
    /// `pattern_metrics::translate_to_group_keys` and in
    /// `ProseBudgetMatrix::budget_for` to fall back to a safe default
    /// rather than panicking.
    pub const fn valid_for(&self, group: PickGroup) -> bool {
        match self {
            Self::Foundational | Self::Incidental => matches!(group, PickGroup::Structure),
            Self::Configured => matches!(group, PickGroup::Derives),
            Self::Marker => matches!(group, PickGroup::Derives | PickGroup::Traits),
            Self::Lifecycle => matches!(group, PickGroup::Traits),
            Self::FreeFn | Self::Macro => matches!(group, PickGroup::Utilities),
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

    /// What: parse a wire-form group prefix back to a `PickGroup`.
    /// Returns `None` on an unknown prefix.
    ///
    /// Why: emit-time consumers (`crate::emit::instance::candidate_instances`,
    /// `crate::emit::orientation`) receive each pick as a
    /// `<group_wire>:<name>` string and need the typed group back
    /// to dispatch matrix lookups + per-group rendering.
    ///
    /// Where: called when splitting each pick's string key into
    /// `(group, name)`; the name remainder feeds the instance
    /// lookup and the group feeds the prose-budget matrix.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "traits" => Some(Self::Traits),
            "trait_functions" => Some(Self::TraitFunctions),
            "structure" => Some(Self::Structure),
            "implementation_functions" => Some(Self::ImplementationFunctions),
            "derives" => Some(Self::Derives),
            "utilities" => Some(Self::Utilities),
            "globals" => Some(Self::Globals),
            _ => None,
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

/// What: a single picks-data pattern - a fieldful enum tagging an
/// architectural pick by its `PickGroup` with the group-shaped
/// payload. `Traits` / `Structure` / `Derives` / `Utilities` /
/// `Globals` carry a single name; `ImplementationFunctions` /
/// `TraitFunctions` carry the `(outer, inner)` pair (e.g. `World::new`,
/// `_::update`) so the compound key is modeled structurally rather
/// than re-split from a string.
///
/// Why: typing the key lets walker + carry + picker code be typed
/// end-to-end against `Pattern` (match on the variant) rather than
/// building `format!("group:name")` strings and re-parsing them via
/// `split_once` at each consumer. Per `mem:developer
/// {rule:rust_enum_kind_mirror}` the bare discriminant is the Copy
/// `PickGroup` via [`Pattern::kind`].
///
/// Where: keys `ItemFacts::carries` / `WorkspaceFacts::carries`, the
/// picker's significance sets, and `EnrichedEntry::pattern`. The wire
/// form (`<group_wire>:<name>`) is unchanged from the prior stringly
/// keys, so facts.json / orientation.md stay byte-compatible. Custom
/// serde emits/parses that wire form, so a map keyed by `Pattern`
/// serializes as `{"structure:Foo": ...}`; `Ord` follows the wire
/// string so `BTreeMap<Pattern, _>` keeps the prior alphabetical key
/// order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Traits(String),
    TraitFunctions { outer: String, inner: String },
    Structure(String),
    ImplementationFunctions { outer: String, inner: String },
    Derives(String),
    Utilities(String),
    Globals(String),
}

impl Pattern {
    /// What: the bare discriminant (the Copy kind-mirror) for this
    /// pattern, per `{rule:rust_enum_kind_mirror}`. Lets bucketing
    /// code group by `PickGroup` without the payload.
    pub fn kind(&self) -> PickGroup {
        match self {
            Self::Traits(_) => PickGroup::Traits,
            Self::TraitFunctions { .. } => PickGroup::TraitFunctions,
            Self::Structure(_) => PickGroup::Structure,
            Self::ImplementationFunctions { .. } => PickGroup::ImplementationFunctions,
            Self::Derives(_) => PickGroup::Derives,
            Self::Utilities(_) => PickGroup::Utilities,
            Self::Globals(_) => PickGroup::Globals,
        }
    }

    /// What: the `<name>` portion of the wire form - the bare name for
    /// the single-name variants, `<outer>::<inner>` for the compound
    /// ones.
    pub fn name(&self) -> String {
        match self {
            Self::Traits(n)
            | Self::Structure(n)
            | Self::Derives(n)
            | Self::Utilities(n)
            | Self::Globals(n) => n.clone(),
            Self::TraitFunctions { outer, inner }
            | Self::ImplementationFunctions { outer, inner } => {
                format!("{}::{}", outer, inner)
            }
        }
    }

    /// What: string-convertible constructors for the single-name and
    /// compound variants. Keep call sites + tests terse.
    pub fn traits(name: impl Into<String>) -> Self {
        Self::Traits(name.into())
    }
    pub fn structure(name: impl Into<String>) -> Self {
        Self::Structure(name.into())
    }
    pub fn derives(name: impl Into<String>) -> Self {
        Self::Derives(name.into())
    }
    pub fn utilities(name: impl Into<String>) -> Self {
        Self::Utilities(name.into())
    }
    pub fn globals(name: impl Into<String>) -> Self {
        Self::Globals(name.into())
    }
    pub fn impl_fn(outer: impl Into<String>, inner: impl Into<String>) -> Self {
        Self::ImplementationFunctions {
            outer: outer.into(),
            inner: inner.into(),
        }
    }
    pub fn trait_fn(outer: impl Into<String>, inner: impl Into<String>) -> Self {
        Self::TraitFunctions {
            outer: outer.into(),
            inner: inner.into(),
        }
    }

    /// What: build a `Pattern` from a `(PickGroup, name)` pair, where
    /// `name` is the wire `<name>` portion (split on the first `::`
    /// for the compound groups; the picker never emits a compound name
    /// without `::`, so the empty-inner fallback is defensive).
    pub fn from_group_name(group: PickGroup, name: &str) -> Self {
        match group {
            PickGroup::Traits => Self::Traits(name.to_string()),
            PickGroup::Structure => Self::Structure(name.to_string()),
            PickGroup::Derives => Self::Derives(name.to_string()),
            PickGroup::Utilities => Self::Utilities(name.to_string()),
            PickGroup::Globals => Self::Globals(name.to_string()),
            PickGroup::ImplementationFunctions => {
                let (outer, inner) = split_outer_inner(name);
                Self::ImplementationFunctions { outer, inner }
            }
            PickGroup::TraitFunctions => {
                let (outer, inner) = split_outer_inner(name);
                Self::TraitFunctions { outer, inner }
            }
        }
    }

    /// What: parse a wire-form `<group_wire>:<name>` string into a
    /// `Pattern`. `None` on an unknown group prefix or a missing `:`.
    pub fn from_wire(s: &str) -> Option<Self> {
        let (group_wire, name) = s.split_once(':')?;
        let group = PickGroup::from_wire(group_wire)?;
        Some(Self::from_group_name(group, name))
    }
}

/// What: split a compound `<outer>::<inner>` name on the FIRST `::`.
/// No `::` yields the whole string as `outer` with an empty `inner`
/// (defensive; not emitted by the picker).
fn split_outer_inner(name: &str) -> (String, String) {
    match name.split_once("::") {
        Some((o, i)) => (o.to_string(), i.to_string()),
        None => (name.to_string(), String::new()),
    }
}

impl std::fmt::Display for Pattern {
    /// What: renders as `<group_wire>:<name>` (e.g.
    /// `structure:Component`, `derives:Clone`,
    /// `implementation_functions:World::new`,
    /// `implementation_functions:_::update`). Byte-compatible with the
    /// prior stringly key shape.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let g = self.kind().wire();
        match self {
            Self::Traits(n)
            | Self::Structure(n)
            | Self::Derives(n)
            | Self::Utilities(n)
            | Self::Globals(n) => write!(f, "{}:{}", g, n),
            Self::TraitFunctions { outer, inner }
            | Self::ImplementationFunctions { outer, inner } => {
                write!(f, "{}:{}::{}", g, outer, inner)
            }
        }
    }
}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pattern {
    /// What: orders by the `Display` wire form so `BTreeMap<Pattern,
    /// _>` keys serialize in the alphabetical order the prior
    /// `BTreeMap<String, _>` produced. Consistent with `Eq` because
    /// `Display` is injective over the pattern space (distinct
    /// patterns render to distinct wire strings; outer / inner are
    /// single idents that never contain `::`).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl serde::Serialize for Pattern {
    /// What: serializes as the wire string so a map keyed by `Pattern`
    /// emits JSON object keys (`{"structure:Foo": ...}`).
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Pattern {
    /// What: parses the wire string back to a `Pattern` (the JSON
    /// object-key boundary). Rejects an unknown group prefix.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct PatternVisitor;
        impl<'v> serde::de::Visitor<'v> for PatternVisitor {
            type Value = Pattern;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a <group_wire>:<name> pattern string")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> std::result::Result<Pattern, E> {
                Pattern::from_wire(s)
                    .ok_or_else(|| E::custom(format!("invalid pattern wire form: {}", s)))
            }
        }
        d.deserialize_str(PatternVisitor)
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
        assert_eq!(
            Pattern::structure("Component").to_string(),
            "structure:Component"
        );
        assert_eq!(Pattern::derives("Clone").to_string(), "derives:Clone");
        assert_eq!(
            Pattern::impl_fn("World", "new").to_string(),
            "implementation_functions:World::new"
        );
        assert_eq!(
            Pattern::impl_fn("_", "update").to_string(),
            "implementation_functions:_::update"
        );
        assert_eq!(
            Pattern::trait_fn("Handler", "handle").to_string(),
            "trait_functions:Handler::handle"
        );
        // kind() is the Copy discriminant mirror.
        assert_eq!(Pattern::structure("X").kind(), PickGroup::Structure);
        assert_eq!(
            Pattern::impl_fn("A", "b").kind(),
            PickGroup::ImplementationFunctions
        );
        // wire round-trip via from_wire.
        for p in [
            Pattern::structure("Component"),
            Pattern::derives("Clone"),
            Pattern::impl_fn("World", "new"),
            Pattern::impl_fn("_", "update"),
            Pattern::trait_fn("Handler", "handle"),
        ] {
            assert_eq!(
                Pattern::from_wire(&p.to_string()),
                Some(p.clone()),
                "round-trip {}",
                p
            );
        }
    }

    #[test]
    fn subform_wire_round_trip() {
        for sf in [
            SubForm::Foundational,
            SubForm::Incidental,
            SubForm::Configured,
            SubForm::Marker,
            SubForm::Lifecycle,
            SubForm::FreeFn,
            SubForm::Macro,
        ] {
            let json = serde_json::to_string(&sf).expect("serialize");
            let parsed: SubForm = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(sf, parsed);
            assert_eq!(json.trim_matches('"'), sf.wire());
        }
    }

    #[test]
    fn subform_valid_for_partition() {
        assert!(SubForm::Foundational.valid_for(PickGroup::Structure));
        assert!(SubForm::Incidental.valid_for(PickGroup::Structure));
        assert!(!SubForm::Foundational.valid_for(PickGroup::Traits));
        assert!(SubForm::Configured.valid_for(PickGroup::Derives));
        assert!(!SubForm::Configured.valid_for(PickGroup::Traits));
        assert!(SubForm::Marker.valid_for(PickGroup::Derives));
        assert!(SubForm::Marker.valid_for(PickGroup::Traits));
        assert!(!SubForm::Marker.valid_for(PickGroup::Structure));
        assert!(SubForm::Lifecycle.valid_for(PickGroup::Traits));
        assert!(!SubForm::Lifecycle.valid_for(PickGroup::Derives));
        assert!(SubForm::FreeFn.valid_for(PickGroup::Utilities));
        assert!(SubForm::Macro.valid_for(PickGroup::Utilities));
        assert!(!SubForm::FreeFn.valid_for(PickGroup::Structure));
        for g in [
            PickGroup::ImplementationFunctions,
            PickGroup::TraitFunctions,
            PickGroup::Globals,
        ] {
            for sf in [
                SubForm::Foundational,
                SubForm::Incidental,
                SubForm::Configured,
                SubForm::Marker,
                SubForm::Lifecycle,
                SubForm::FreeFn,
                SubForm::Macro,
            ] {
                assert!(
                    !sf.valid_for(g),
                    "{:?} should not be valid for {:?}",
                    sf,
                    g
                );
            }
        }
    }

    #[test]
    fn pattern_hashable() {
        use std::collections::HashMap;
        let mut map: HashMap<Pattern, usize> = HashMap::new();
        map.insert(Pattern::structure("Component"), 1);
        map.insert(Pattern::derives("Clone"), 2);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&Pattern::structure("Component")), Some(&1));
    }

    #[test]
    fn pattern_serde_as_map_key() {
        // Custom serde emits/parses the wire string, so a BTreeMap keyed
        // by Pattern serializes as a JSON object with string keys and
        // round-trips back. Locks the facts.json carries wire shape.
        use std::collections::BTreeMap;
        let mut m: BTreeMap<Pattern, u32> = BTreeMap::new();
        m.insert(Pattern::structure("Foo"), 1);
        m.insert(Pattern::impl_fn("Bar", "new"), 2);
        m.insert(Pattern::impl_fn("_", "update"), 3);
        let json = serde_json::to_string(&m).expect("serialize");
        assert!(json.contains("\"structure:Foo\":1"), "json: {json}");
        assert!(
            json.contains("\"implementation_functions:Bar::new\":2"),
            "json: {json}"
        );
        assert!(
            json.contains("\"implementation_functions:_::update\":3"),
            "json: {json}"
        );
        let back: BTreeMap<Pattern, u32> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.get(&Pattern::structure("Foo")), Some(&1));
        assert_eq!(back.get(&Pattern::impl_fn("Bar", "new")), Some(&2));
        assert_eq!(back.get(&Pattern::impl_fn("_", "update")), Some(&3));
    }
}
