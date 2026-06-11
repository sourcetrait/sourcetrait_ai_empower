use crate::*;

/// What: one workspace-ADOPTED foreign root - a foreign crate the
/// workspace re-exports as part of its own API surface, with the
/// adoption forms observed (namespace / glob / leaf), the lock-pinned
/// version, and the registry checkout that enumeration reads.
///
/// Why: the workspace-adopted design rule (the_user;
/// notes/know_rust/working/03_picks_data.md): a re-export is the
/// author DECLARING the foreign items an aspect of their own API -
/// cross-pollination, not pollution. Identity never conflates: the
/// root carries its own provenance (package, version, registry
/// source). std/core/alloc never adopt (the reader's floor);
/// type-rooted re-exports are variant lifts, not adoption;
/// example-file and scaffolding-crate re-export sites are not API.
///
/// Where: produced by `resolve_adopted_roots` at characterize time;
/// written to `know_rust_adopted.json`; the enumeration slice reads
/// `checkout` to walk the adopted crate's pub surface.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdoptedRoot {
    /// The written root binding (underscore-normalized spelling).
    pub root: String,
    /// The cargo package name (hyphenated form when the lock says so).
    pub package: String,
    /// Lock-pinned version; empty when the lock carries no entry.
    pub version: String,
    /// All lock versions when the graph pins more than one (the
    /// first is `version`; ambiguity is recorded, not resolved).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    /// Registry checkout dir for (package, version); None when the
    /// registry src does not carry it (graceful degradation:
    /// namespace residue, 5F still names the root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout: Option<String>,
    /// Whole-crate namespace adoption (`pub use glam;`).
    #[serde(default)]
    pub namespace: bool,
    /// Glob adoptions: the path remainder below the root whose `*`
    /// is re-exported ("" = the crate root, `pub use glam::*;`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub globs: Vec<String>,
    /// Leaf adoptions: (source item name, exposed binding).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leaves: Vec<(String, String)>,
    /// Adopting re-export rows observed (site count).
    pub sites: usize,
}

/// What: resolve the workspace's ADOPTED foreign roots from its
/// `pub use` facts: filter to adoption-eligible re-export sites,
/// classify each row's form (namespace / glob / leaf), translate the
/// root through the consuming crate's dependency renames, pin the
/// version from Cargo.lock, and locate the registry checkout.
///
/// Why: slice 1 of the workspace-adopted unit - the roots table is
/// the contract the enumeration slice consumes. Exclusions per the
/// design rule + the probe findings: std/core/alloc never adopt;
/// lang roots (crate/self/super) are self-references; workspace
/// crates (packages, lib renames, dep renames, vendored units) are
/// origin, not adoption; TYPE-rooted re-exports
/// (`pub use Alignment::*;`) are enum-variant lifts; example-file
/// sites and scaffolding-crate sites are not API; doc(hidden)
/// re-exports are not surface.
///
/// Where: called from `characterize::run::characterize` after the
/// decl channel; output written as `know_rust_adopted.json`.
pub fn resolve_adopted_roots(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    workspace_root: &Path,
) -> Vec<AdoptedRoot> {
    let vocab = ResolveVocab::from_crates(crates);
    let scaffolding: HashSet<String> = crates
        .iter()
        .filter(|(n, i)| is_scaffolding(n, i))
        .map(|(n, _)| n.clone())
        .collect();
    let lock = lock_versions(workspace_root);
    let lang_roots = ["crate", "self", "super"];
    let std_roots = ["std", "core", "alloc"];

    // root (underscore-normalized) -> accumulating entry.
    let mut roots: BTreeMap<String, AdoptedRoot> = BTreeMap::new();

    for u in &all_facts.uses {
        if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if u.get("doc_hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let file = u.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if is_example_path(file) {
            continue;
        }
        let using = u.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if using.is_empty() || scaffolding.contains(using) {
            continue;
        }
        let Some(path) = u.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        let parsed = parse_use_leaves(path);
        if parsed.root.is_empty() || lang_roots.contains(&parsed.root.as_str()) {
            continue;
        }
        if std_roots.contains(&parsed.root.as_str()) {
            continue;
        }
        // Type-rooted re-exports (`pub use Alignment::*;`) lift enum
        // variants of a WORKSPACE type - not adoption.
        if parsed.root.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            continue;
        }
        // Workspace crates (packages + lib renames) are origin.
        let root_norm = parsed.root.replace('-', "_");
        if vocab.resolve_root(using, &parsed.root).is_some() {
            continue;
        }
        // The consuming crate's dependency rename translates the
        // binding to its cargo package; a rename onto a workspace
        // member is still origin.
        let package = crates
            .get(using)
            .and_then(|ci| {
                ci.renames
                    .iter()
                    .find(|(b, _)| b == &parsed.root || b.replace('-', "_") == root_norm)
                    .map(|(_, p)| p.clone())
            })
            .unwrap_or_else(|| parsed.root.clone());
        if crates.contains_key(&package) || crates.contains_key(&package.replace('-', "_")) {
            continue;
        }

        let stripped = path.trim().strip_prefix("::").unwrap_or(path.trim());
        let crate_level = !stripped.contains("::");
        let entry = roots.entry(root_norm.clone()).or_insert_with(|| AdoptedRoot {
            root: root_norm.clone(),
            package: package.clone(),
            version: String::new(),
            versions: Vec::new(),
            checkout: None,
            namespace: false,
            globs: Vec::new(),
            leaves: Vec::new(),
            sites: 0,
        });
        entry.sites += 1;
        if crate_level {
            entry.namespace = true;
        }
        for leaf in &parsed.leaves {
            match leaf {
                UseLeaf::Glob { parent } => {
                    // The glob's module path below the root: the
                    // parent segment names the hop; "" marks the
                    // crate root (`pub use glam::*;` has the root
                    // itself as parent).
                    let below = match parent.as_deref() {
                        Some(p) if p == parsed.root => String::new(),
                        Some(p) => p.to_string(),
                        None => String::new(),
                    };
                    if !entry.globs.contains(&below) {
                        entry.globs.push(below);
                    }
                }
                UseLeaf::Named { binding, source, .. } => {
                    if crate_level {
                        continue;
                    }
                    let src = source.clone().unwrap_or_else(|| binding.clone());
                    let pair = (src, binding.clone());
                    if !entry.leaves.contains(&pair) {
                        entry.leaves.push(pair);
                    }
                }
            }
        }
    }

    // Version pin + registry checkout per root.
    let registry_src = cargo_registry_src();
    let mut out: Vec<AdoptedRoot> = Vec::new();
    for (_, mut r) in roots {
        let candidates = [r.package.clone(), r.package.replace('_', "-")];
        for cand in &candidates {
            if let Some(vs) = lock.get(cand) {
                r.package = cand.clone();
                r.versions = vs.clone();
                r.version = vs.first().cloned().unwrap_or_default();
                break;
            }
        }
        if !r.version.is_empty() {
            if let Some(reg) = &registry_src {
                let dir_name = format!("{}-{}", r.package, r.version);
                for idx in fs::read_dir(reg).into_iter().flatten().flatten() {
                    let cand = idx.path().join(&dir_name);
                    if cand.is_dir() {
                        r.checkout = Some(cand.display().to_string());
                        break;
                    }
                }
            }
        }
        out.push(r);
    }
    out
}

/// What: parse the workspace `Cargo.lock` into package -> sorted
/// version list (a graph can pin multiple versions of one package).
///
/// Why: lock-as-pin is the standing version convention; the adopted
/// root's enumeration must read the EXACT source rev the workspace
/// builds against.
///
/// Where: called by `resolve_adopted_roots`; absent or unparseable
/// locks degrade to empty (no checkout resolution).
fn lock_versions(workspace_root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let Ok(text) = fs::read_to_string(workspace_root.join("Cargo.lock")) else {
        return out;
    };
    let mut name: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l == "[[package]]" {
            name = None;
            continue;
        }
        if let Some(rest) = l.strip_prefix("name = ") {
            name = Some(rest.trim_matches('"').to_string());
        } else if let Some(rest) = l.strip_prefix("version = ") {
            if let Some(n) = name.take() {
                out.entry(n).or_default().push(rest.trim_matches('"').to_string());
            }
        }
    }
    for vs in out.values_mut() {
        vs.sort();
        vs.dedup();
    }
    out
}

/// What: the cargo registry extracted-source root
/// (`$CARGO_HOME/registry/src`), when it exists.
///
/// Why: adopted-crate enumeration reads pinned checkouts from the
/// box's own registry cache - the workspace's declared dependency
/// surface, no network.
///
/// Where: called by `resolve_adopted_roots`.
fn cargo_registry_src() -> Option<PathBuf> {
    let home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".cargo")))?;
    let p = home.join("registry").join("src");
    p.is_dir().then_some(p)
}
