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
    /// Workspace crates whose re-exports adopt this root - the
    /// intra/inter split's pivot for adopted items (an adopting
    /// crate's own usage is intra; other crates' usage through the
    /// surface is inter) and the through-adopter credit set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adopting_crates: Vec<String>,
    /// The adopted crate's pub-reachable module-level surface (kind /
    /// name / shortest public chain), enumerated from the registry
    /// checkout through the decl channel's reachability machinery.
    /// Populated for glob / namespace adoptions with a checkout;
    /// leaf-only roots need no enumeration (their items are named).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface: Vec<SurfaceItem>,
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
            adopting_crates: Vec::new(),
            surface: Vec::new(),
        });
        entry.sites += 1;
        if !entry.adopting_crates.iter().any(|c| c == using) {
            entry.adopting_crates.push(using.to_string());
        }
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

/// What: enumerate the pub-reachable surface of every adopted root
/// that warrants it - glob or namespace adoption with a registry
/// checkout. The checkout is walked by the items walker and read
/// through the decl channel's reachability machinery (`public_surface`),
/// so glob-of-glob closures, leaf re-export lifts, and
/// privacy-piercing hops inside the ADOPTED crate resolve exactly
/// like workspace crates.
///
/// Why: `pub use glam::*;` exposes whatever glam's own re-export
/// graph lifts to its root - only glam's source answers that.
/// Leaf-only adoptions skip enumeration (their items are named in
/// the adopting row).
///
/// Where: called by `characterize::run::characterize` after
/// `resolve_adopted_roots`.
pub fn enumerate_adopted_surfaces(roots: &mut [AdoptedRoot]) {
    for r in roots.iter_mut() {
        if !(r.namespace || !r.globs.is_empty()) {
            continue;
        }
        let Some(checkout) = r.checkout.clone() else {
            continue;
        };
        let checkout_path = PathBuf::from(&checkout);
        if !checkout_path.is_dir() {
            continue;
        }
        let items = scan_workspace(&checkout_path);
        let mut wf = WorkspaceFacts::default();
        let krate = r.package.replace('-', "_");
        let inject = |list: Vec<serde_json::Value>| -> Vec<serde_json::Value> {
            list.into_iter()
                .map(|mut v| {
                    if let Some(o) = v.as_object_mut() {
                        o.insert("crate".into(), serde_json::Value::from(krate.clone()));
                    }
                    v
                })
                .collect()
        };
        wf.fns = inject(items.fns.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        wf.consts = inject(items.consts.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        wf.types = inject(items.types.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        wf.traits = inject(items.traits.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        wf.mods = inject(items.mods.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        wf.uses = inject(items.uses.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect());
        let mut crates_map: indexmap::IndexMap<String, CrateInfo> = indexmap::IndexMap::new();
        crates_map.insert(
            krate.clone(),
            CrateInfo {
                dir: ".".to_string(),
                deps: Vec::new(),
                has_bin: false,
                has_lib: true,
                keywords: Vec::new(),
                categories: Vec::new(),
                description: String::new(),
                version: r.version.clone(),
                lib_name: None,
                renames: Vec::new(),
                unit: String::new(),
            },
        );
        r.surface = public_surface(&wf, &crates_map);
    }
}

/// What: mint pattern_metrics keys for workspace-ADOPTED items -
/// the eligibility slice. Glob-adopted items (surface entries whose
/// shortest chain equals the glob's path), namespace-adopted items
/// (the whole surface), and leaf-adopted items (matched in the
/// surface by source name) become first-class keys: bare
/// `structure:`/`traits:` for types, `implementation_functions:` /
/// `globals:` pairs for fns/consts (outer = the chain tail, else
/// the adopted root). Zero counts; is_pub; `adopted` carries
/// `<package>@<version>`.
///
/// Why: the workspace-adopted design rule - re-exported foreign
/// items are the workspace's own API surface and must be
/// pick-eligible with true provenance. Collision rules (the P3
/// refinement): a TRUE workspace module-level decl (module_path
/// Some, non-example) shadows adoption; macro-token PSEUDO-decls
/// (module_path None - bevy's impl_reflect! rows for the glam
/// types) yield - an existing key whose name has no true decl is
/// RE-ATTRIBUTED to the adopted identity, keeping its counts (real
/// workspace usage of the adopted item). NF5 parity and the
/// fn-main rule hold at mint time.
///
/// Where: called by `characterize::run::characterize` after
/// `decl_api_channel` (existing keys visible for re-attribution).
pub fn adopted_channel(
    adopted: &[AdoptedRoot],
    all_facts: &WorkspaceFacts,
    ast_usages: Option<&UsageFacts>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
) {
    // TRUE module-level workspace decls (the shadow set): types +
    // traits rows with module_path SOME from non-example files.
    let mut true_decls: HashSet<String> = HashSet::new();
    for list in [&all_facts.types, &all_facts.traits] {
        for t in list.iter() {
            if t.get("module_path").and_then(|v| v.as_str()).is_none() {
                continue;
            }
            if is_example_path(t.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                continue;
            }
            if let Some(n) = t.get("name").and_then(|v| v.as_str()) {
                true_decls.insert(n.to_string());
            }
        }
    }

    let mut fresh: Vec<(String, usize, String, String)> = Vec::new();
    for (root_idx, root) in adopted.iter().enumerate() {
        if root.surface.is_empty() {
            continue;
        }
        let provenance = format!("{}@{}", root.package, root.version);
        // Which surface items the adoption forms expose.
        let exposed: Vec<&SurfaceItem> = root
            .surface
            .iter()
            .filter(|s| {
                if root.namespace {
                    return true;
                }
                if root.globs.iter().any(|g| {
                    let gp: Vec<&str> = if g.is_empty() {
                        Vec::new()
                    } else {
                        g.split("::").collect()
                    };
                    s.chain.len() == gp.len()
                        && s.chain.iter().zip(gp.iter()).all(|(a, b)| a == b)
                }) {
                    return true;
                }
                root.leaves.iter().any(|(src, _)| src == &s.name)
            })
            .collect();
        for item in exposed {
            if item.kind == "fn" && item.name == "main" {
                continue;
            }
            let outer = item
                .chain
                .last()
                .cloned()
                .unwrap_or_else(|| root.root.clone());
            let key_wire = match item.kind.as_str() {
                "fn" => Pattern::impl_fn(&outer, &item.name).to_string(),
                "const" | "static" => {
                    Pattern::globals(format!("{}::{}", outer, item.name)).to_string()
                }
                "trait" => Pattern::traits(&item.name).to_string(),
                // struct / enum / union / type aliases.
                _ => Pattern::structure(&item.name).to_string(),
            };
            // NF5 parity: adopted mints respect the substring filter.
            if should_skip_pattern(&key_wire, calibration) {
                continue;
            }
            let bare_type = matches!(item.kind.as_str(), "fn" | "const" | "static") == false;
            if bare_type && true_decls.contains(&item.name) {
                // Workspace-origin shadows adoption.
                continue;
            }
            match metrics.get_mut(&key_wire) {
                Some(m) => {
                    // Re-attribution: the existing key's name has no
                    // true workspace decl (pseudo-decl or defn-null
                    // residue) - the adopted identity wins; counts
                    // stay (real workspace usage of this item).
                    m.adopted = Some(provenance.clone());
                    m.defining_crate = Some(root.root.clone());
                    m.is_pub = true;
                }
                None => {
                    // Fresh mint: zero counts now; the counting pass
                    // below credits workspace usage sites.
                    metrics.insert(
                        key_wire.clone(),
                        PatternMetric {
                            defining_crate: Some(root.root.clone()),
                            intra_count: 0,
                            inter_count: 0,
                            inter_ratio: 0.0,
                            is_pub: true,
                            example_count: serde_json::Value::from(0),
                            curated_example_count: 0,
                            sub_form: None,
                            adopted: Some(provenance.clone()),
                        },
                    );
                    fresh.push((key_wire, root_idx, item.kind.clone(), item.name.clone()));
                }
            }
        }
    }

    count_adopted_usage(&fresh, adopted, all_facts, ast_usages, metrics);
}

/// What: credit WORKSPACE usage sites to freshly-minted adopted keys
/// (the internal-usage counting slice). A site credits when its
/// written qualifier or import binding resolves to the adopted root
/// directly, or to a workspace crate that ADOPTS that root (usage
/// through the re-exporting surface). The intra/inter split pivots
/// on the adopting set: an adopting crate's own usage is intra;
/// any other crate's usage through the surface is inter - the
/// architectural-reliance signal (the_user: bevy relies on glam's
/// types internally and externally).
///
/// Why: adoption's significance must not depend on consumer demand
/// alone - the workspace's own reliance is the primary signal
/// (working/03, internal-usage-counts ruling). Only FRESH mints
/// count here: re-attributed keys keep their existing counts (the
/// same sites already credited them through the pseudo-decl
/// attribution; re-counting would double).
///
/// Where: tail of `adopted_channel`. Example-dir sites never count
/// (units 10/12); evidence tallies stay untouched.
fn count_adopted_usage(
    fresh: &[(String, usize, String, String)],
    adopted: &[AdoptedRoot],
    all_facts: &WorkspaceFacts,
    ast_usages: Option<&UsageFacts>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
) {
    if fresh.is_empty() {
        return;
    }
    let import_maps = build_import_bindings(all_facts.uses.iter().filter_map(|u| {
        Some((
            u.get("file").and_then(|v| v.as_str())?,
            u.get("path").and_then(|v| v.as_str())?,
        ))
    }));
    // name -> (key, root index) per kind family.
    let mut type_keys: HashMap<&str, (&str, usize)> = HashMap::new();
    let mut trait_keys: HashMap<&str, (&str, usize)> = HashMap::new();
    let mut fn_keys: HashMap<&str, (&str, usize)> = HashMap::new();
    for (key, root_idx, kind, name) in fresh {
        match kind.as_str() {
            "trait" => {
                trait_keys.insert(name.as_str(), (key.as_str(), *root_idx));
            }
            "fn" => {
                fn_keys.insert(name.as_str(), (key.as_str(), *root_idx));
            }
            "const" | "static" => {}
            _ => {
                type_keys.insert(name.as_str(), (key.as_str(), *root_idx));
            }
        }
    }
    // A site credits root R when its resolved root IS R's binding or
    // a workspace crate adopting R.
    let site_credits_root = |file: &str, name: &str, qualifier: Option<&str>, r: &AdoptedRoot| -> bool {
        if let Some(q) = qualifier {
            let qn = q.replace('-', "_");
            return qn == r.root || r.adopting_crates.iter().any(|c| c.replace('-', "_") == qn);
        }
        if let Some(b) = import_maps.get(file).and_then(|m| m.get(name)) {
            let rn = b.root.replace('-', "_");
            return rn == r.root
                || r.adopting_crates.iter().any(|c| c.replace('-', "_") == rn);
        }
        false
    };
    let mut credit = |key: &str, using: &str, r: &AdoptedRoot| {
        if let Some(m) = metrics.get_mut(key) {
            if r.adopting_crates.iter().any(|c| c == using) {
                m.intra_count += 1;
            } else {
                m.inter_count += 1;
            }
            let total = m.intra_count + m.inter_count;
            m.inter_ratio = if total > 0 {
                let raw = m.inter_count as f64 / total as f64;
                format!("{:.3}", raw).parse().unwrap_or(raw)
            } else {
                0.0
            };
        }
    };

    // Trait kinds: impl + derive facts.
    for (facts, field) in [(&all_facts.impls, "trait"), (&all_facts.derives, "trait")] {
        for f in facts.iter() {
            let Some(n) = f.get(field).and_then(|v| v.as_str()) else {
                continue;
            };
            let Some((key, ri)) = trait_keys.get(n) else {
                continue;
            };
            let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
            if is_example_path(file) {
                continue;
            }
            let using = f.get("crate").and_then(|v| v.as_str()).unwrap_or("");
            if using.is_empty() {
                continue;
            }
            let r = &adopted[*ri];
            if site_credits_root(file, n, f.get("qualifier").and_then(|v| v.as_str()), r) {
                credit(key, using, r);
            }
        }
    }
    // Type kinds: factory-call outers (main stream) + AST sig/field/
    // alias idents.
    for f in all_facts.type_usages.iter() {
        let Some(nm) = f.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let outer = nm.split("::").next().unwrap_or(nm);
        let Some((key, ri)) = type_keys.get(outer) else {
            continue;
        };
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if is_example_path(file) {
            continue;
        }
        let using = f.get("crate").and_then(|v| v.as_str()).unwrap_or("");
        if using.is_empty() {
            continue;
        }
        let r = &adopted[*ri];
        if site_credits_root(file, outer, f.get("qualifier").and_then(|v| v.as_str()), r) {
            credit(key, using, r);
        }
    }
    if let Some(usages) = ast_usages {
        // The three ident-shaped AST streams are distinct types;
        // flatten to (ident, file, qualifier). The AST wires carry
        // no crate, so these credit conservatively as inter
        // (cross-crate reliance is the signal that matters;
        // adopting-crate self-use mostly rides the qualified +
        // imported streams above).
        let mut ident_sites: Vec<(&str, &str, Option<&str>)> = Vec::new();
        for e in &usages.ast_fn_sig_usages {
            ident_sites.push((e.ident.as_str(), e.file.as_str(), e.qualifier.as_deref()));
        }
        for e in &usages.ast_field_usages {
            ident_sites.push((e.ident.as_str(), e.file.as_str(), e.qualifier.as_deref()));
        }
        for e in &usages.ast_type_alias_usages {
            ident_sites.push((e.ident.as_str(), e.file.as_str(), e.qualifier.as_deref()));
        }
        for (ident, file, qualifier) in ident_sites {
            let Some((key, ri)) = type_keys.get(ident) else {
                continue;
            };
            if is_example_path(file) {
                continue;
            }
            let r = &adopted[*ri];
            if site_credits_root(file, ident, qualifier, r) {
                credit(key, "", r);
            }
        }
        for e in &usages.ast_fn_call_usages {
            let Some((key, ri)) = fn_keys.get(e.name.as_str()) else {
                continue;
            };
            if is_example_path(&e.file) {
                continue;
            }
            let r = &adopted[*ri];
            if site_credits_root(&e.file, &e.name, e.qualifier.as_deref(), r) {
                credit(key, "", r);
            }
        }
    }
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
