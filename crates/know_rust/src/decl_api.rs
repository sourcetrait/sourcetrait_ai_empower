use crate::*;

/// What: the declaration-driven API channel. For every module-level
/// `pub fn` whose declaration is pub-REACHABLE (its hard module
/// chain, or at least one `pub use` soft binding, runs through
/// all-pub, non-doc(hidden) modules), ensure a pair-shaped
/// pattern_metrics key exists (`implementation_functions:
/// <outer>::<fn>`) and record the item's alternate binding outers as
/// pair aliases for the demand matcher. Returns the alias map
/// (rendered-pair wire name -> alias outers).
///
/// Why: pure consumer-facing API (zero internal usage) produces no
/// usage-driven key at all - the largest real miss class in the
/// consumer audits. Identity = the hard declaration path; names =
/// soft bindings (the_user's both-model ruling): the KEY is one
/// spelling (an existing usage spelling wins, else the canonical
/// binding by curated-lift), and every other public binding rides as
/// a matcher-consultable alias so a demand through any spelling is
/// served.
///
/// Where: called by `characterize::run::characterize` after
/// `compute_pattern_metrics`; the alias map lands in
/// `WorkspaceFacts::pair_aliases` and the minted zero-count metrics
/// rows await the consumer-weight score term.
pub(crate) fn decl_api_channel(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
) -> BTreeMap<String, Vec<String>> {
    let mods = ModIndex::build(all_facts, crates);
    let decls = collect_decls(all_facts, crates, &mods);
    let bindings = collect_bindings(all_facts, crates, &mods);

    let mut pair_aliases: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for decl in &decls {
        // Public binding set: the hard chain when reachable, plus
        // every reachable soft binding matched to this decl.
        let mut public_chains: Vec<Vec<String>> = Vec::new();
        if decl.hard_public {
            public_chains.push(decl.chain.clone());
        }
        if let Some(soft) = bindings.get(&(decl.krate.clone(), decl.name.clone())) {
            for b in soft {
                // Parent-based disambiguation: a binding whose use
                // path named a source parent attaches only to decls
                // whose hard chain ends in that parent; a binding
                // with no parent attaches when the decl name is
                // unique in the crate (handled by the caller's
                // uniqueness map inside collect_bindings).
                let attaches = match &b.source_parent {
                    Some(p) => decl.chain.last().map(|s| s == p).unwrap_or(false),
                    None => b.unique_in_crate,
                };
                if attaches {
                    public_chains.push(b.chain.clone());
                }
            }
        }
        if public_chains.is_empty() {
            continue;
        }
        // Outer per chain: last module segment, else the crate's
        // root binding (lib rename when one exists).
        let root_outer = crates
            .get(&decl.krate)
            .and_then(|c| c.lib_name.clone())
            .unwrap_or_else(|| decl.krate.replace('-', "_"));
        let mut outers: Vec<String> = public_chains
            .iter()
            .map(|c| c.last().cloned().unwrap_or_else(|| root_outer.clone()))
            .collect();
        outers.sort();
        outers.dedup();

        // One key per item: an existing usage-driven spelling wins;
        // else the canonical binding (shortest public chain,
        // deterministic tie-break).
        let existing = outers
            .iter()
            .find(|o| metrics.contains_key(&Pattern::impl_fn((*o).clone(), decl.name.clone()).to_string()));
        let canonical_outer = match existing {
            Some(o) => o.clone(),
            None => {
                let mut ranked = public_chains.clone();
                ranked.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.join("::").cmp(&b.join("::"))));
                ranked
                    .first()
                    .and_then(|c| c.last().cloned())
                    .unwrap_or_else(|| root_outer.clone())
            }
        };
        let key = Pattern::impl_fn(canonical_outer.clone(), decl.name.clone());
        let key_wire = key.to_string();
        // NF5 parity: a minted decl key must not resurrect a family
        // the substring filter dropped from the usage streams
        // (sourcetrait_common's `util::_var_for_test_*` class).
        if should_skip_pattern(&key_wire, calibration) {
            continue;
        }
        if !metrics.contains_key(&key_wire) {
            metrics.insert(
                key_wire,
                PatternMetric {
                    defining_crate: Some(decl.krate.clone()),
                    intra_count: 0,
                    inter_count: 0,
                    inter_ratio: 0.0,
                    is_pub: true,
                    example_count: serde_json::Value::from(0),
                    curated_example_count: 0,
                    sub_form: None,
                },
            );
        }
        let alias_set = pair_aliases
            .entry(format!("{}::{}", canonical_outer, decl.name))
            .or_default();
        for o in outers {
            if o != canonical_outer {
                alias_set.insert(o);
            }
        }
        // The hard parent is always a consultable spelling even when
        // the hard chain is not itself public (a demand written
        // through a deep path still means this item).
        if let Some(hp) = decl.chain.last() {
            if hp != &canonical_outer {
                alias_set.insert(hp.clone());
            }
        }
    }

    pair_aliases
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect()
}

/// What: a module-level pub fn declaration candidate - crate, name,
/// full hard module chain (file-derived + inline), and whether that
/// hard chain is pub-reachable.
struct DeclCandidate {
    krate: String,
    name: String,
    chain: Vec<String>,
    hard_public: bool,
}

/// What: one soft binding - the public path chain a `pub use` gives
/// the name, the use path's source-parent segment (for matching the
/// binding to the right same-named decl), and whether the bound name
/// is unique among the crate's decl candidates (the no-parent match
/// rule).
struct SoftBinding {
    chain: Vec<String>,
    source_parent: Option<String>,
    unique_in_crate: bool,
}

/// What: per-(crate, module chain) visibility index built from mods
/// facts; answers "is this chain all-pub and not doc(hidden)?".
///
/// Why: pub-reachability is the gate that keeps pub-in-private-module
/// fns (tokio's mpsc/chan.rs `pub fn channel`) out of the API
/// channel; raw `pub` keywords overcount.
///
/// Where: built once per characterize by `decl_api_channel`.
struct ModIndex {
    vis: HashMap<(String, String), (bool, bool)>,
}

impl ModIndex {
    fn build(
        all_facts: &WorkspaceFacts,
        crates: &indexmap::IndexMap<String, CrateInfo>,
    ) -> Self {
        let mut vis: HashMap<(String, String), (bool, bool)> = HashMap::new();
        for m in &all_facts.mods {
            let (Some(name), Some(krate), Some(file)) = (
                m.get("name").and_then(|v| v.as_str()),
                m.get("crate").and_then(|v| v.as_str()),
                m.get("file").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let Some(dir) = crates.get(krate).map(|c| c.dir.as_str()) else {
                continue;
            };
            let Some(fchain) = file_module_chain(dir, file) else {
                continue;
            };
            let inline = m
                .get("module_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut chain = fchain;
            chain.extend(split_chain(inline));
            chain.push(name.to_string());
            let is_pub = m
                .get("visibility")
                .and_then(|v| v.as_str())
                .map(|v| v == "pub")
                .unwrap_or(false);
            let hidden = m
                .get("doc_hidden")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // First decl wins; duplicate mod decls (cfg variants)
            // keep the more permissive read deterministically.
            let entry = vis
                .entry((krate.to_string(), chain.join("::")))
                .or_insert((is_pub, hidden));
            entry.0 |= is_pub;
            entry.1 &= hidden;
        }
        Self { vis }
    }

    /// True when every prefix of `chain` is a pub, non-hidden mod
    /// (the crate root itself is public). Unknown segments are NOT
    /// public (conservative: macro-generated or unscanned decls).
    fn chain_public(&self, krate: &str, chain: &[String]) -> bool {
        for i in 1..=chain.len() {
            match self.vis.get(&(krate.to_string(), chain[..i].join("::"))) {
                Some((true, false)) => {}
                _ => return false,
            }
        }
        true
    }
}

/// What: collect module-level pub fn decl candidates with their hard
/// chains + reachability.
fn collect_decls(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    mods: &ModIndex,
) -> Vec<DeclCandidate> {
    let mut out = Vec::new();
    for f in &all_facts.fns {
        let (Some(name), Some(krate), Some(file)) = (
            f.get("name").and_then(|v| v.as_str()),
            f.get("crate").and_then(|v| v.as_str()),
            f.get("file").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let Some(mp) = f.get("module_path").and_then(|v| v.as_str()) else {
            continue;
        };
        if f.get("visibility").and_then(|v| v.as_str()) != Some("pub") {
            continue;
        }
        if f.get("doc_hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        // `fn main` is a language ENTRY POINT, not consumable API
        // (the_user-granted noise rule); the decl channel never
        // mints it (cosmic-epoch's per-binary crate-root mains).
        if name == "main" {
            continue;
        }
        if !is_src_file(file) {
            continue;
        }
        let Some(dir) = crates.get(krate).map(|c| c.dir.as_str()) else {
            continue;
        };
        let Some(mut chain) = file_module_chain(dir, file) else {
            continue;
        };
        chain.extend(split_chain(mp));
        let hard_public = mods.chain_public(krate, &chain);
        out.push(DeclCandidate {
            krate: krate.to_string(),
            name: name.to_string(),
            chain,
            hard_public,
        });
    }
    out
}

/// What: collect reachable soft bindings from `pub use` facts, keyed
/// by (crate, source name).
fn collect_bindings(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    mods: &ModIndex,
) -> HashMap<(String, String), Vec<SoftBinding>> {
    // Name uniqueness among decl candidates per crate, for the
    // no-parent binding match rule.
    let mut name_counts: HashMap<(String, String), usize> = HashMap::new();
    for f in &all_facts.fns {
        if f.get("module_path").and_then(|v| v.as_str()).is_none() {
            continue;
        }
        if let (Some(n), Some(k)) = (
            f.get("name").and_then(|v| v.as_str()),
            f.get("crate").and_then(|v| v.as_str()),
        ) {
            *name_counts.entry((k.to_string(), n.to_string())).or_default() += 1;
        }
    }

    let mut out: HashMap<(String, String), Vec<SoftBinding>> = HashMap::new();
    for u in &all_facts.uses {
        if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        if u.get("doc_hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let (Some(path), Some(krate), Some(file)) = (
            u.get("path").and_then(|v| v.as_str()),
            u.get("crate").and_then(|v| v.as_str()),
            u.get("file").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        if !is_src_file(file) {
            continue;
        }
        let Some(dir) = crates.get(krate).map(|c| c.dir.as_str()) else {
            continue;
        };
        let Some(mut site_chain) = file_module_chain(dir, file) else {
            continue;
        };
        site_chain.extend(split_chain(
            u.get("module_path").and_then(|v| v.as_str()).unwrap_or(""),
        ));
        // The binding's public path = the use site's chain + the
        // bound name; reachable when the site chain is all-pub (the
        // `pub use` itself is pub by the reexport flag).
        if !mods.chain_public(krate, &site_chain) {
            continue;
        }
        let parsed = parse_use_leaves(path);
        if parsed.root.is_empty() {
            continue;
        }
        for leaf in &parsed.leaves {
            let UseLeaf::Named { binding, source, parent } = leaf else {
                continue;
            };
            let src_name = source
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| binding.clone());
            // The binding's MODULE chain is the use site's chain (the
            // bound name lives directly in that module) - symmetric
            // with DeclCandidate::chain, which also excludes the fn
            // name itself.
            let chain = site_chain.clone();
            let unique = name_counts
                .get(&(krate.to_string(), src_name.clone()))
                .copied()
                .unwrap_or(0)
                == 1;
            out.entry((krate.to_string(), src_name))
                .or_default()
                .push(SoftBinding {
                    chain,
                    source_parent: parent.clone(),
                    unique_in_crate: unique,
                });
        }
    }
    out
}

/// What: derive the module chain a file contributes from its path
/// relative to its crate dir: `src/lib.rs` / `src/main.rs` -> [],
/// `src/foo.rs` -> [foo], `src/foo/mod.rs` -> [foo],
/// `src/a/b.rs` -> [a, b], `src/bin/x.rs` -> [] (binary root).
/// `None` for files outside `src/` (build scripts etc. are not API
/// module space).
///
/// Why: the hard declaration path = this file chain + the inline mod
/// chain; both reachability and canonical selection walk it.
///
/// Where: called by the decl channel's index/candidate/binding
/// builders.
pub(crate) fn file_module_chain(crate_dir: &str, file: &str) -> Option<Vec<String>> {
    let norm = file.replace('\\', "/");
    let rel = if crate_dir == "." || crate_dir.is_empty() {
        norm.as_str()
    } else {
        norm.strip_prefix(&format!("{}/", crate_dir.trim_end_matches('/')))?
    };
    let rest = rel.strip_prefix("src/")?;
    let stem = rest.strip_suffix(".rs")?;
    let mut segs: Vec<&str> = stem.split('/').collect();
    match segs.as_slice() {
        ["lib"] | ["main"] => return Some(Vec::new()),
        _ => {}
    }
    if segs.first() == Some(&"bin") {
        return Some(Vec::new());
    }
    if segs.last() == Some(&"mod") {
        segs.pop();
    }
    Some(segs.into_iter().map(String::from).collect())
}

/// What: split an inline module_path wire value ("", "a", "a::b")
/// into segments.
fn split_chain(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split("::").map(String::from).collect()
    }
}
