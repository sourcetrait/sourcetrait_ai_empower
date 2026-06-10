use crate::*;

/// What: one leaf of a flattened use-tree string. `Named` carries the
/// in-scope BINDING name plus the imported item's own SOURCE name
/// (`X as Y` binds Y with source X; `{self}` binds the parent segment;
/// a plain leaf binds itself). `Glob` marks a `*` import, which is
/// unresolvable per-ident.
///
/// Why: the two prior use-tree grammars each kept half the picture -
/// pattern_metrics' parser returned bindings only (enough for origin
/// resolution) while measure_demand's returned (binding, source) pairs
/// (needed for rename translation back to source names). One leaf
/// shape carries both so capture and demand parse imports identically.
///
/// Where: produced by `parse_use_leaves`; consumed by
/// `build_import_bindings` / `build_reexport_index` and by
/// `measure_demand::trace::demand_report`'s import-surface loop.
pub(crate) enum UseLeaf {
    Named {
        binding: String,
        source: Option<String>,
    },
    Glob,
}

/// What: the parse of one flattened use-tree string - the path's ROOT
/// segment (first segment, `r#` prefix stripped; empty when the path
/// is empty) plus the expanded leaves.
///
/// Why: every consumer of the use grammar needs the (root, leaves)
/// pair together: the root decides which crate the bindings resolve
/// to, the leaves decide which names enter scope.
///
/// Where: returned by `parse_use_leaves`.
pub(crate) struct UseParse {
    pub(crate) root: String,
    pub(crate) leaves: Vec<UseLeaf>,
}

/// What: expand one flattened use-tree string (the
/// `flatten_use_tree` wire form: `a::b::{C, d as E, *}`) into its
/// root segment plus `UseLeaf` entries. Handles brace groups
/// (recursively, with the prefix's last segment as the `self`
/// parent), `as` renames (`self as X` binds X with the parent as
/// source), bare `self` (binds the parent segment), and globs. A
/// bare top-level `self` with no parent yields no leaf.
///
/// Why: this grammar is the single source of truth for reading
/// import surfaces - per-file import maps, facade re-export
/// indexes, and the consumer-side demand trace all read use facts
/// through it, so a grammar fix lands everywhere at once.
///
/// Where: called by `build_import_bindings` + `build_reexport_index`
/// here and by `measure_demand::trace::demand_report` for demand
/// extraction + the crate-wide alias map.
pub(crate) fn parse_use_leaves(path: &str) -> UseParse {
    fn leaves(s: &str, parent_last: Option<&str>, out: &mut Vec<UseLeaf>) {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        if s == "*" {
            out.push(UseLeaf::Glob);
            return;
        }
        if let Some(brace) = s.find('{') {
            let prefix = s[..brace].trim_end_matches("::").trim();
            let prefix_last = prefix.rsplit("::").next().filter(|p| !p.is_empty());
            let inner = &s[brace + 1..s.rfind('}').unwrap_or(s.len())];
            for piece in split_top_commas(inner) {
                leaves(&piece, prefix_last.or(parent_last), out);
            }
            return;
        }
        if let Some((base, renamed)) = s.rsplit_once(" as ") {
            let binding = renamed.trim().to_string();
            let base = base.trim();
            let source = if base == "self" {
                parent_last.map(String::from)
            } else {
                Some(base.rsplit("::").next().unwrap_or(base).to_string())
            };
            out.push(UseLeaf::Named { binding, source });
            return;
        }
        if s == "self" {
            if let Some(p) = parent_last {
                out.push(UseLeaf::Named {
                    binding: p.to_string(),
                    source: Some(p.to_string()),
                });
            }
            return;
        }
        let leaf = s.rsplit("::").next().unwrap_or(s).trim();
        // Full-path glob (`a::b::*`, no brace group): the bare-`*`
        // check above only sees brace PIECES, so the suffix form
        // must divert here or the glob parses as Named("*") - inert
        // on the capture side (no ident is ever `*`) but the demand
        // trace misreported it as a missing NAME instead of a glob.
        if leaf == "*" {
            out.push(UseLeaf::Glob);
            return;
        }
        if !leaf.is_empty() {
            out.push(UseLeaf::Named {
                binding: leaf.to_string(),
                source: Some(leaf.to_string()),
            });
        }
    }
    let trimmed = path.trim();
    // The root is the first segment of the BASE path: a
    // single-segment rename (`core_k as ck`) carries its ` as `
    // suffix inside the first `::`-chunk and must be stripped, or
    // the binding resolves to garbage (External).
    let root = trimmed
        .split("::")
        .next()
        .unwrap_or("")
        .split(" as ")
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches("r#")
        .to_string();
    let mut out = Vec::new();
    if !root.is_empty() {
        leaves(trimmed, None, &mut out);
    }
    UseParse { root, leaves: out }
}

/// What: one per-file import binding - the path ROOT the binding
/// resolves through (raw first segment; crate-name normalization is
/// the caller's lookup concern) and the imported item's SOURCE name
/// when the binding is a rename (`None` when binding == source).
///
/// Why: origin resolution needs the root; rename translation needs
/// the source. Carrying both in the shared map shape lets
/// pattern_metrics and measure_demand consume one import surface.
///
/// Where: values of `ImportMaps`; also the value shape of
/// measure_demand's crate-wide alias map.
pub(crate) struct ImportBinding {
    pub(crate) root: String,
    pub(crate) source: Option<String>,
}

/// What: per-file import surface - file path -> (binding ->
/// `ImportBinding`), first-wins per (file, binding).
pub(crate) type ImportMaps = HashMap<String, HashMap<String, ImportBinding>>;

/// What: build the per-file import maps from (file, use-path) pairs.
/// Globs are skipped (unresolvable per-ident); the first binding of a
/// name in a file wins, matching shadow-free Rust import semantics
/// closely enough for name-level attribution.
///
/// Why: item path resolution is the assumed mode of attribution
/// (working/02): a usage site's identifier resolves through its
/// file's use-imports to an origin crate. Both the capture side
/// (pattern_metrics) and the demand side (measure_demand) build this
/// surface from their own use facts via the same function.
///
/// Where: called from `compute_pattern_metrics` over facts.json
/// `uses` entries and from `measure_demand::trace::demand_report`
/// over the consumer's typed `UseEntry` list.
pub(crate) fn build_import_bindings<'a, I>(uses: I) -> ImportMaps
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut by_file: ImportMaps = HashMap::new();
    for (file, path) in uses {
        let parsed = parse_use_leaves(path);
        if parsed.root.is_empty() {
            continue;
        }
        let entry = by_file.entry(file.to_string()).or_default();
        for leaf in &parsed.leaves {
            if let UseLeaf::Named { binding, source } = leaf {
                entry
                    .entry(binding.clone())
                    .or_insert_with(|| ImportBinding {
                        root: parsed.root.clone(),
                        source: source.clone(),
                    });
            }
        }
    }
    by_file
}

/// What: the facade re-export index. `leaf_reexports` holds each
/// crate's re-exported BINDING names (`pub use core::X as Y` exposes
/// Y); `ns_closure` holds, per crate, the transitive set of in-repo
/// crates whose whole NAMESPACE the crate re-exports - a
/// crate-level `pub use iced;` / `pub use iced_core as core;` /
/// root-glob `pub use iced::*;` creates an edge, and edges chain
/// (libcosmic -> iced -> iced_core).
///
/// Why: a workspace facade re-exporting another crate's item is the
/// same item, and that holds for whole-crate namespace re-exports
/// too: a site spelled `cosmic::iced::Foo` resolves
/// Workspace(libcosmic) while Foo defines in a fork crate; the
/// name-level leaf rule alone covered only itemwise re-exports.
/// Module-level re-exports (`pub use k::m;`) stay out of scope -
/// name-level granularity, consistent with the system's attribution.
///
/// Where: built in `compute_pattern_metrics` via
/// `build_facade_index`; judged by `site_credits` and the free-fn
/// facade redirect.
pub(crate) struct FacadeIndex {
    pub(crate) leaf_reexports: HashMap<String, HashSet<String>>,
    pub(crate) ns_closure: HashMap<String, HashSet<String>>,
}

impl FacadeIndex {
    /// What: true when `via_crate` re-exports `ident` by name OR
    /// wholesale re-exports (transitively) the namespace of
    /// `defining_crate`.
    ///
    /// Why: the single verdict both facade forms feed; callers stay
    /// agnostic of which form carried the site.
    ///
    /// Where: called from `site_credits` for the Workspace /
    /// SelfCrate arms.
    pub(crate) fn credits(&self, via_crate: &str, defining_crate: &str, ident: &str) -> bool {
        self.leaf_reexports
            .get(via_crate)
            .map(|s| s.contains(ident))
            .unwrap_or(false)
            || self
                .ns_closure
                .get(via_crate)
                .map(|s| s.contains(defining_crate))
                .unwrap_or(false)
    }
}

/// What: build the facade index from (crate, use-path) pairs of
/// `pub use` facts: binding leaves into `leaf_reexports`, and
/// crate-level namespace edges - a non-group path whose base (before
/// any `as` rename) is a single segment resolving through the
/// vocabulary to an in-repo crate, or a root-glob `k::*` - expanded
/// to a transitive closure.
///
/// Why: one construction point for both facade forms keeps the
/// credit rule's evidence uniform; resolving edge targets through
/// `ResolveVocab` means lib renames participate (`pub use cosmic;`
/// would edge to package libcosmic).
///
/// Where: called from `compute_pattern_metrics` over
/// reexport-flagged `uses` facts.
pub(crate) fn build_facade_index<'a, I>(reexports: I, vocab: &ResolveVocab) -> FacadeIndex
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut leaf_reexports: HashMap<String, HashSet<String>> = HashMap::new();
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for (krate, path) in reexports {
        let parsed = parse_use_leaves(path);
        if parsed.root.is_empty() {
            continue;
        }
        for leaf in &parsed.leaves {
            if let UseLeaf::Named { binding, .. } = leaf {
                leaf_reexports
                    .entry(krate.to_string())
                    .or_default()
                    .insert(binding.clone());
            }
        }
        let trimmed = path.trim();
        if trimmed.contains('{') {
            continue;
        }
        let base = trimmed
            .rsplit_once(" as ")
            .map(|(b, _)| b.trim())
            .unwrap_or(trimmed);
        let is_crate_level = !base.contains("::");
        let is_root_glob = base
            .split_once("::")
            .map(|(_, rest)| rest.trim() == "*")
            .unwrap_or(false);
        if is_crate_level || is_root_glob {
            if let Some(canonical) = vocab.resolve_root(krate, &parsed.root) {
                if canonical != krate {
                    edges
                        .entry(krate.to_string())
                        .or_default()
                        .insert(canonical.clone());
                }
            }
        }
    }
    // Transitive closure (cycle-safe BFS): libcosmic -> iced ->
    // iced_core chains into one reachable set per crate.
    let mut ns_closure: HashMap<String, HashSet<String>> = HashMap::new();
    for start in edges.keys() {
        let mut reach: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = edges
            .get(start)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        while let Some(k) = queue.pop() {
            if k == *start || !reach.insert(k.clone()) {
                continue;
            }
            if let Some(next) = edges.get(&k) {
                queue.extend(next.iter().cloned());
            }
        }
        ns_closure.insert(start.clone(), reach);
    }
    FacadeIndex {
        leaf_reexports,
        ns_closure,
    }
}

/// What: the bindings -> crate vocabulary: every name a source root
/// can use for an in-repo crate, reconciled to the canonical package
/// name. Global bindings are package names plus `[lib]` rename names
/// of every crate (host and units); on top sits a per-consuming-crate
/// overlay of dependency renames (`foo = { package = "bar" }` binds
/// `foo` for that crate only).
///
/// Why: names are BINDINGS to identities, never identities (the_user
/// ruling, 2026-06-10). The prior vocabulary keyed package names
/// only, so any repo with a renamed lib (`libcosmic` -> `cosmic`) or
/// a renamed dependency resolved those roots to External and lost
/// the sites. Hyphen normalization is just lexing, not
/// reconciliation.
///
/// Where: built once per characterize in `compute_pattern_metrics`
/// from the discovery's `CrateInfo`s; consulted by
/// `resolve_ident_origin` / `resolve_site_origin` and the free-fn
/// synthesis qualifier check.
pub(crate) struct ResolveVocab {
    members: HashMap<String, String>,
    renames_by_crate: HashMap<String, HashMap<String, String>>,
}

impl ResolveVocab {
    /// What: build the vocabulary from the discovered crates: package
    /// bindings + lib-rename bindings into the global table
    /// (first-wins with a stderr note on cross-crate collisions,
    /// deterministic in cargo's package order), and each crate's
    /// dependency renames into the per-crate overlay.
    ///
    /// Why: one construction point keeps the binding rules uniform
    /// for every consumer of root resolution.
    ///
    /// Where: called from `compute_pattern_metrics`.
    pub(crate) fn from_crates(crates: &indexmap::IndexMap<String, CrateInfo>) -> Self {
        let mut members: HashMap<String, String> = HashMap::new();
        let bind = |binding: &str, canonical: &str, members: &mut HashMap<String, String>| {
            let key = binding.replace('-', "_");
            if let Some(existing) = members.get(&key) {
                if existing != canonical {
                    eprintln!(
                        "[resolution] binding collision: `{}` -> `{}` kept, `{}` ignored",
                        key, existing, canonical
                    );
                }
                return;
            }
            members.insert(key, canonical.to_string());
        };
        for (pkg, info) in crates {
            bind(pkg, pkg, &mut members);
            if let Some(lib) = &info.lib_name {
                bind(lib, pkg, &mut members);
            }
        }
        let mut renames_by_crate: HashMap<String, HashMap<String, String>> = HashMap::new();
        for (pkg, info) in crates {
            for (binding, dep_pkg) in &info.renames {
                renames_by_crate
                    .entry(pkg.clone())
                    .or_default()
                    .entry(binding.replace('-', "_"))
                    .or_insert_with(|| dep_pkg.clone());
            }
        }
        Self {
            members,
            renames_by_crate,
        }
    }

    /// What: resolve a source ROOT binding as seen from
    /// `using_crate` to the canonical package name - the using
    /// crate's dependency-rename overlay first, then the global
    /// package/lib bindings. `None` when the root binds no in-repo
    /// crate.
    ///
    /// Why: dependency renames are per-consuming-crate by cargo
    /// semantics; the overlay-then-global order mirrors how rustc
    /// resolves the extern prelude for that crate.
    ///
    /// Where: called by `resolve_ident_origin` (import roots),
    /// `resolve_site_origin` (written qualifiers), and the free-fn
    /// synthesis.
    pub(crate) fn resolve_root(&self, using_crate: &str, root: &str) -> Option<&String> {
        let key = root.replace('-', "_");
        if let Some(overlay) = self.renames_by_crate.get(using_crate) {
            if let Some(canonical) = overlay.get(&key) {
                return Some(canonical);
            }
        }
        self.members.get(&key)
    }
}

/// What: where a usage-site identifier resolves to, per the using
/// file's imports. `SelfCrate` covers `crate::` / `self::` / `super::`
/// import paths plus prelude-shadowing local declarations.
///
/// Why: the workspace-origin rule holds at usage sites through this
/// taxonomy - std / external resolutions were never capture
/// candidates, and the demand side registers only affirmative
/// workspace resolutions.
///
/// Where: returned by `resolve_ident_origin` / `resolve_site_origin`;
/// judged by `site_credits` on the capture side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdentOrigin {
    Std,
    External,
    Workspace(String),
    SelfCrate,
    Unresolved,
}

/// What: Rust 2021/2024 prelude names. An identifier with NO import
/// hit that matches one of these resolves to std per language
/// semantics (the prelude is an implicit import) - unless the using
/// crate itself declares the name (shadowing).
///
/// Why: this is resolution DATA, not a capture rule - the
/// workspace-origin rule alone governs capture (working/02). Without
/// prelude resolution, name-keyed fallback hands every bare `Result`
/// / `Vec` / `Box` usage to any workspace alias of the same name (R7
/// topic k: tokio structure:Result topping the architecture set).
pub(crate) const STD_PRELUDE: &[&str] = &[
    "Box", "Vec", "String", "ToString", "ToOwned", "Clone", "Copy", "Debug", "Default",
    "PartialEq", "Eq", "PartialOrd", "Ord", "Hash", "Iterator", "IntoIterator",
    "DoubleEndedIterator", "ExactSizeIterator", "Extend", "Option", "Some", "None",
    "Result", "Ok", "Err", "From", "Into", "TryFrom", "TryInto", "AsRef", "AsMut",
    "Send", "Sync", "Sized", "Unpin", "Drop", "Fn", "FnMut", "FnOnce", "FromIterator",
];

/// What: resolve `ident` as used in `file` (owned by `using_crate`)
/// to its origin. Import map first; then prelude (with same-crate
/// shadow check via `local_decl_crates`); else Unresolved.
///
/// Why: makes the workspace-origin rule hold at usage sites - std /
/// external resolutions were never candidates for capture, so the
/// callers skip those sites (no separate stdlib rule; working/02).
///
/// Where: called from `compute_pattern_metrics`' counting loop +
/// synthesis paths and from `resolve_site_origin`'s module-root
/// branch.
pub(crate) fn resolve_ident_origin(
    file: &str,
    ident: &str,
    using_crate: &str,
    import_maps: &ImportMaps,
    vocab: &ResolveVocab,
    local_decl_crates: &HashMap<String, HashSet<String>>,
) -> IdentOrigin {
    if let Some(map) = import_maps.get(file) {
        if let Some(binding) = map.get(ident) {
            return match binding.root.as_str() {
                "std" | "core" | "alloc" => IdentOrigin::Std,
                "crate" | "self" | "super" => IdentOrigin::SelfCrate,
                other => match vocab.resolve_root(using_crate, other) {
                    Some(canonical) => IdentOrigin::Workspace(canonical.clone()),
                    None => IdentOrigin::External,
                },
            };
        }
    }
    if STD_PRELUDE.contains(&ident) {
        let shadowed = local_decl_crates
            .get(ident)
            .map(|crates| crates.contains(using_crate))
            .unwrap_or(false);
        if !shadowed {
            return IdentOrigin::Std;
        }
        return IdentOrigin::SelfCrate;
    }
    IdentOrigin::Unresolved
}

/// What: resolve a usage site that may carry an explicit path
/// QUALIFIER (the written root of a longer path: `std::env::args` ->
/// "std"; `git2::Status::INDEX_NEW` -> "git2"; `io::Error` -> "io").
/// A keyword / member root resolves directly per language semantics;
/// a module-name root resolves through the file's imports (use
/// std::io; io::Error -> Std). Without a qualifier, falls back to
/// `resolve_ident_origin` on the bare ident.
///
/// Why: the walkers record the last two path segments; the qualifier
/// preserves the explicit root that the crate-local Unresolved
/// fallback was silently absorbing (R7 topics k/l residue:
/// env::args / io::stdout / structure:Error classes).
///
/// Where: called from `compute_pattern_metrics`' counting loop, its
/// pub_type / method_ref / assoc-const / variant-ref synthesis paths,
/// and its example-evidence gate.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_site_origin(
    file: &str,
    ident: &str,
    qualifier: Option<&str>,
    using_crate: &str,
    import_maps: &ImportMaps,
    vocab: &ResolveVocab,
    local_decl_crates: &HashMap<String, HashSet<String>>,
    local_mod_crates: &HashMap<String, HashSet<String>>,
) -> IdentOrigin {
    if let Some(q) = qualifier {
        return match q {
            "std" | "core" | "alloc" => IdentOrigin::Std,
            "crate" | "self" | "super" => IdentOrigin::SelfCrate,
            other => {
                if let Some(canonical) = vocab.resolve_root(using_crate, other) {
                    IdentOrigin::Workspace(canonical.clone())
                } else {
                    // Module-name root: resolve the module ident
                    // through this file's imports first. An
                    // unimported root is crate-local only when the
                    // using crate declares a module of that name;
                    // otherwise the root is an external crate.
                    match resolve_ident_origin(
                        file,
                        other,
                        using_crate,
                        import_maps,
                        vocab,
                        local_decl_crates,
                    ) {
                        IdentOrigin::Unresolved => {
                            let is_local_mod = local_mod_crates
                                .get(other)
                                .map(|crates| crates.contains(using_crate))
                                .unwrap_or(false);
                            if is_local_mod {
                                IdentOrigin::Unresolved
                            } else {
                                IdentOrigin::External
                            }
                        }
                        resolved => resolved,
                    }
                }
            }
        };
    }
    resolve_ident_origin(
        file,
        ident,
        using_crate,
        import_maps,
        vocab,
        local_decl_crates,
    )
}

/// What: true when a usage site is creditable toward a pattern whose
/// declaration lives in `defining_crate`: workspace-resolved to that
/// crate (directly or through a workspace facade crate that
/// re-exports the name), self-crate-resolved while using it, or
/// unresolved (the crate-local-unimported fallback). Std / external /
/// unrelated-workspace resolutions are not credited.
///
/// Why: this is the capture side's verdict on a resolved origin. The
/// demand side intentionally judges differently (affirmative
/// resolution only - Unresolved is NOT demand), so the verdict stays
/// separate from the shared resolution substrate above.
///
/// Where: called from `compute_pattern_metrics`' counting loop and
/// synthesis paths.
pub(crate) fn site_credits(
    origin: &IdentOrigin,
    using_crate: &str,
    defining_crate: &str,
    ident: &str,
    facades: &FacadeIndex,
) -> bool {
    match origin {
        IdentOrigin::Std | IdentOrigin::External => false,
        IdentOrigin::Workspace(c) => {
            c == defining_crate || facades.credits(c, defining_crate, ident)
        }
        IdentOrigin::SelfCrate => {
            using_crate == defining_crate
                || facades.credits(using_crate, defining_crate, ident)
        }
        IdentOrigin::Unresolved => true,
    }
}
