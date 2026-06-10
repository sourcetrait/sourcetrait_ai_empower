/// What: serde-serializable overlay structure mirroring
/// rustdoc_overlay.py's `overlay` dict shape. The top-level fields
/// serialize in declaration order via serde to match Python's dict
/// insertion order for byte-equal JSON output.
///
/// Why: rustdoc_overlay.py writes its result as
/// `json.dumps(overlay, indent=2)` so the consumer can read a
/// human-friendly indented JSON. The Rust port maintains that exact
/// shape so existing the_user verification tooling stays compatible.
///
/// Where: built by `crate::rustdoc_overlay::run::rustdoc_overlay`
/// after the reconcile pass; serialized via `serde_json::to_string_
/// pretty` and written to `<orientation_dir>/rustdoc_overlay.json`.
#[derive(serde::Serialize, Debug, Clone)]
pub struct Overlay {
    pub status: &'static str,
    pub format_version: u64,
    pub macro_generated: Vec<MacroGenerated>,
    pub reexports: Vec<Reexport>,
    pub null_span_items: Vec<NullSpanItem>,
    pub disagreements: Vec<Disagreement>,
    /// What: rustdoc's canonical paths for FUNCTION items - fn name
    /// -> the `::`-joined path list from the rustdoc JSON `paths`
    /// table (multiple same-name fns keep all paths).
    ///
    /// Why: rustdoc computes reachability-true canonical paths; the
    /// section validates the decl channel's structural canonical
    /// choice and supplies extra matcher alias spellings. Keys stay
    /// overlay-INDEPENDENT (the structural fallback selects them)
    /// so pair passes without an overlay key identically.
    ///
    /// Where: built in `reconcile` from the rustdoc `paths` table.
    pub paths: std::collections::BTreeMap<String, Vec<String>>,
    /// What: names of PUBLIC trait / type items per rustdoc's
    /// post-expansion index (visibility == "public"; kinds trait /
    /// struct / enum / union / type alias).
    ///
    /// Why: macro-expansion-invisible VISIBILITY (bevy's
    /// define_label! emits `pub trait ScheduleLabel` from a
    /// name-only invocation) leaves the floor's is_pub false and
    /// blocks public-set eligibility no token recovery can fix;
    /// rustdoc sees the expansion, so overlay-bearing emits backfill
    /// is_pub from these sets.
    ///
    /// Where: built in `reconcile` from the rustdoc index; consumed
    /// by `emit::picker::compute_significance_sets` via the
    /// vis-backfill thread.
    #[serde(default)]
    pub pub_traits: Vec<String>,
    #[serde(default)]
    pub pub_types: Vec<String>,
    /// What: the workspace package this overlay documented (the
    /// resolved `-p` argument).
    ///
    /// Why: vis backfill may need to attribute an entry the floor
    /// could not attribute at all (no declaration fact exists for a
    /// fully macro-generated item); rustdoc attests both the
    /// public visibility AND the owning package.
    ///
    /// Where: set by `rustdoc_overlay` from the resolved package;
    /// consumed by the emit-side `VisBackfill` as the
    /// defining-crate fallback.
    #[serde(default)]
    pub package: Option<String>,
}

/// What: one macro-generated impl entry - the trait name, a null span
/// (because rustdoc reports null for blanket / synthesized / macro
/// items), and a fixed "no span" note.
#[derive(serde::Serialize, Debug, Clone)]
pub struct MacroGenerated {
    #[serde(rename = "trait")]
    pub trait_name: String,
    pub span: Option<String>,
    pub note: &'static str,
}

/// What: one re-export entry - the export's name and its source path
/// (rustdoc resolves the target through `pub use`).
#[derive(serde::Serialize, Debug, Clone)]
pub struct Reexport {
    pub name: Option<String>,
    pub source: Option<serde_json::Value>,
}

/// What: one null-span item - rustdoc emits null spans for re-
/// exports, blanket / synthesized impls, and macro-generated items.
/// The overlay flags them rather than dropping.
#[derive(serde::Serialize, Debug, Clone)]
pub struct NullSpanItem {
    pub name: String,
    pub kind: Option<String>,
}

/// What: one disagreement entry partitioning rustdoc-only impl traits
/// into either std-blanket coverage (informational) or user-domain
/// macro-only impls (the loud signal of a registration-macro seam).
#[derive(serde::Serialize, Debug, Clone)]
pub struct Disagreement {
    pub kind: &'static str,
    pub traits: Vec<String>,
    pub note: &'static str,
}

/// What: format versions this overlay has been verified against. The
/// rustdoc JSON schema changes between nightlies; reconcile() degrades
/// (errors in the hard-nightly Rust port) when an unknown version
/// shows up rather than risk misreading.
pub const FORMAT_VERSION_MIN: u64 = 26;

/// What: highest format version verified through rustdoc_overlay.py;
/// the format-57 rename of `trait.name` to `trait.path` is handled in
/// reconcile.
pub const FORMAT_VERSION_MAX: u64 = 57;

/// What: standard-library blanket / auto-impl traits. The compiler
/// synthesizes these for every type satisfying their bounds, so they
/// always land in the disagreement list under the naive logic. Split
/// out as an informational std-blanket-coverage disagreement so the
/// loud `impls_only_in_rustdoc` signal surfaces only user-domain
/// macro registration.
pub const STD_BLANKET_TRAITS: &[&str] = &[
    "Any",
    "Borrow",
    "BorrowMut",
    "CloneToUninit",
    "Freeze",
    "From",
    "Into",
    "Receiver",
    "RefUnwindSafe",
    "Send",
    "Sized",
    "Sync",
    "ToOwned",
    "ToString",
    "TryFrom",
    "TryInto",
    "Unpin",
    "UnsafeUnpin",
    "UnwindSafe",
];
