use crate::*;

/// What: the declaration-driven API channel over the full
/// pub-reachable module-level decl surface. Three legs share one
/// reachability substrate (hard chains + leaf soft bindings + glob
/// soft bindings):
///
/// - fns: mint `implementation_functions:<outer>::<fn>` pair keys
///   (zero counts) + pair aliases for the demand matcher.
/// - consts/statics: mint `globals:<outer>::<NAME>` pair keys the
///   same way (ratatui's `symbols::line::*`, bevy's palettes).
/// - types/traits: mint bare `structure:<name>` / `traits:<name>`
///   keys (zero counts) for pub-reachable decls no usage stream
///   ever keyed (bevy's `pub type Write` class).
///
/// Returns the pair-alias map (rendered-pair wire name -> alias
/// outers).
///
/// Why: pure consumer-facing API (zero internal usage) produces no
/// usage-driven key at all - the largest real miss class in the
/// consumer audits, on the fn, const, AND type sides. Identity = the
/// hard declaration path; names = soft bindings (the_user's
/// both-model ruling). A glob re-export at a pub site is a soft
/// binding for every item in the named module - bevy's private
/// `mod system_param;` + `pub use system_param::*;` is the only
/// public path to `pub type Write`. Minted keys carry zero counts
/// and render only when the consumer-weight term scores them
/// (weighted-render-only).
///
/// Where: called by `characterize::run::characterize` after
/// `compute_pattern_metrics`; the alias map lands in
/// `WorkspaceFacts::pair_aliases`.
pub(crate) fn decl_api_channel(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
) -> BTreeMap<String, Vec<String>> {
    let mods = ModIndex::build(all_facts, crates);
    let (bindings, glob_bindings) = collect_bindings(all_facts, crates, &mods);

    let mut pair_aliases: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();

    // fns leg: pair keys under implementation_functions.
    let fn_decls = collect_decl_rows(&all_facts.fns, crates, &mods, true);
    let fn_counts = module_level_name_counts(&[&all_facts.fns]);
    mint_pair_decls(
        &fn_decls,
        &fn_counts,
        &bindings,
        &glob_bindings,
        &mods,
        crates,
        metrics,
        calibration,
        &mut pair_aliases,
        &|outer, name| Pattern::impl_fn(outer, name),
    );

    // consts leg: pair keys under globals.
    let const_decls = collect_decl_rows(&all_facts.consts, crates, &mods, false);
    let const_counts = module_level_name_counts(&[&all_facts.consts]);
    mint_pair_decls(
        &const_decls,
        &const_counts,
        &bindings,
        &glob_bindings,
        &mods,
        crates,
        metrics,
        calibration,
        &mut pair_aliases,
        &|outer, name| Pattern::globals(format!("{}::{}", outer, name)),
    );

    // types leg: bare structure:/traits: keys, reachability-gated.
    let type_counts = module_level_name_counts(&[&all_facts.types, &all_facts.traits]);
    mint_type_decls(
        &all_facts.types,
        crates,
        &mods,
        &bindings,
        &glob_bindings,
        &type_counts,
        metrics,
        calibration,
        &|name| Pattern::structure(name),
    );
    mint_type_decls(
        &all_facts.traits,
        crates,
        &mods,
        &bindings,
        &glob_bindings,
        &type_counts,
        metrics,
        calibration,
        &|name| Pattern::traits(name),
    );

    pair_aliases
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect()
}

/// What: a module-level pub decl candidate - crate, name, full hard
/// module chain (file-derived + inline), and whether that hard chain
/// is pub-reachable.
struct DeclCandidate {
    krate: String,
    name: String,
    chain: Vec<String>,
    hard_public: bool,
}

/// What: one leaf soft binding - the chain a `pub use` gives the
/// name, the use path's source-parent segment (for matching the
/// binding to the right same-named decl), and whether the binding
/// SITE's chain is pub-reachable. Private-site bindings still
/// create virtual LOCATIONS for further re-export hops (tokio's
/// io-util: `pub use copy::copy;` sits in the private io::util,
/// and io/mod.rs's `pub use util::{copy}` hops it public -
/// re-exports pierce privacy; only the final hop's site decides
/// publicness). The no-parent match rule consults the per-leg
/// name-count map.
struct SoftBinding {
    chain: Vec<String>,
    source_parent: Option<String>,
    site_public: bool,
    /// What: true when the use path resolves RELATIVE to its site
    /// (uniform-path and `self::` roots). Site-relative bindings
    /// attach only to locations UNDER their site chain - tokio's
    /// `pub use self::copy::copy;` in fs/ must not attach the
    /// same-parent-named io-util copy decl (the fs::copy/io::copy
    /// conflation). crate::/super::-rooted paths stay tail-only.
    site_relative: bool,
}

/// What: one glob soft binding - a `pub use <path>::*;`. Attaches
/// by splicing decl chains at `parent`; `site_public` marks whether
/// the site chain is pub-reachable (private-site globs still create
/// hop locations).
struct GlobBinding {
    krate: String,
    chain: Vec<String>,
    parent: String,
    site_public: bool,
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

    /// What: the (is_pub, doc_hidden) of the single mod declared at
    /// `chain` (not the full-prefix walk - the glob splice checks
    /// per-level visibility below the hop, where the prefix above
    /// the hop is vouched by the glob site).
    fn vis_of(&self, krate: &str, chain: &[String]) -> Option<(bool, bool)> {
        self.vis
            .get(&(krate.to_string(), chain.join("::")))
            .copied()
    }
}

/// What: collect module-level pub decl candidates with their hard
/// chains + reachability from one fact array (fns or consts share
/// the field shape: name / crate / file / module_path / visibility /
/// doc_hidden). `exclude_main` applies the fn-only entry-point rule.
fn collect_decl_rows(
    rows: &[serde_json::Value],
    crates: &indexmap::IndexMap<String, CrateInfo>,
    mods: &ModIndex,
    exclude_main: bool,
) -> Vec<DeclCandidate> {
    let mut out = Vec::new();
    for f in rows {
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
        if exclude_main && name == "main" {
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

/// What: per-(crate, name) counts of MODULE-LEVEL decls across the
/// given fact arrays - the no-parent soft-binding attach rule's
/// ambiguity guard (a binding with no source parent attaches only
/// when the name is unique among the crate's module-level decls).
fn module_level_name_counts(
    lists: &[&[serde_json::Value]],
) -> HashMap<(String, String), usize> {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for rows in lists {
        for f in rows.iter() {
            if f.get("module_path").and_then(|v| v.as_str()).is_none() {
                continue;
            }
            if let (Some(n), Some(k)) = (
                f.get("name").and_then(|v| v.as_str()),
                f.get("crate").and_then(|v| v.as_str()),
            ) {
                *counts.entry((k.to_string(), n.to_string())).or_default() += 1;
            }
        }
    }
    counts
}

/// What: collect reachable soft bindings from `pub use` facts - leaf
/// bindings keyed by (crate, source name) plus the glob bindings
/// (`pub use <path>::*;` sites).
fn collect_bindings(
    all_facts: &WorkspaceFacts,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    mods: &ModIndex,
) -> (
    HashMap<(String, String), Vec<SoftBinding>>,
    Vec<GlobBinding>,
) {
    let mut out: HashMap<(String, String), Vec<SoftBinding>> = HashMap::new();
    let mut globs: Vec<GlobBinding> = Vec::new();
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
        // The binding's chain is the use site's chain (the bound
        // name lives directly in that module) - symmetric with
        // DeclCandidate::chain. Publicness of the SITE rides the
        // binding; private-site bindings stay collectable as hop
        // locations (re-exports pierce privacy).
        let site_public = mods.chain_public(krate, &site_chain);
        let parsed = parse_use_leaves(path);
        if parsed.root.is_empty() {
            continue;
        }
        let site_relative = parsed.root != "crate" && parsed.root != "super";
        for leaf in &parsed.leaves {
            match leaf {
                UseLeaf::Named { binding, source, parent } => {
                    let src_name = source
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| binding.clone());
                    out.entry((krate.to_string(), src_name))
                        .or_default()
                        .push(SoftBinding {
                            chain: site_chain.clone(),
                            source_parent: parent.clone(),
                            site_public,
                            site_relative,
                        });
                }
                UseLeaf::Glob { parent: Some(p) } => {
                    globs.push(GlobBinding {
                        krate: krate.to_string(),
                        chain: site_chain.clone(),
                        parent: p.clone(),
                        site_public,
                    });
                }
                UseLeaf::Glob { parent: None } => {}
            }
        }
    }
    (out, globs)
}

/// What: the public binding chains for one decl - the hard chain
/// when reachable, every attaching reachable leaf binding, and every
/// matching glob binding.
fn public_chains_for(
    decl: &DeclCandidate,
    name_counts: &HashMap<(String, String), usize>,
    bindings: &HashMap<(String, String), Vec<SoftBinding>>,
    glob_bindings: &[GlobBinding],
    mods: &ModIndex,
) -> Vec<Vec<String>> {
    let mut public_chains: Vec<Vec<String>> = Vec::new();
    if decl.hard_public {
        public_chains.push(decl.chain.clone());
    }
    // Location BFS over the re-export graph. A LOCATION is a module
    // chain where the item's name is in scope; hops are GLOB
    // splices (`pub use m::*;` lifts items + pub sub-modules of m:
    // bevy's system::lifetimeless::Write; suffix segments below the
    // hop must each be pub, non-hidden mods of the REAL tree) and
    // LEAF bindings (parent-matched against the item's known
    // location tails: nushell's root `pub use engine::
    // {NU_VARIABLE_ID}` names the VIRTUAL location). Hops compose
    // (the closure), and they pierce privacy - a binding at a
    // PRIVATE site still creates a hop location (tokio io-util:
    // `pub use copy::copy;` in private io::util, hopped out by
    // io/mod.rs's `pub use util::{copy}`); only hops whose own SITE
    // chain is pub-reachable contribute PUBLIC chains.
    let no_parent_attaches = name_counts
        .get(&(decl.krate.clone(), decl.name.clone()))
        .copied()
        .unwrap_or(0)
        == 1;
    let soft = bindings.get(&(decl.krate.clone(), decl.name.clone()));
    let mut queue: Vec<Vec<String>> = vec![decl.chain.clone()];
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    seen.insert(decl.chain.join("::"));
    while let Some(chain) = queue.pop() {
        for g in glob_bindings {
            if g.krate != decl.krate {
                continue;
            }
            let Some(i) = chain.iter().rposition(|s| s == &g.parent) else {
                continue;
            };
            let suffix_ok = ((i + 1)..chain.len()).all(|j| {
                matches!(
                    mods.vis_of(&decl.krate, &chain[..=j]),
                    Some((true, false))
                )
            });
            if !suffix_ok {
                continue;
            }
            let mut next = g.chain.clone();
            next.extend(chain[(i + 1)..].iter().cloned());
            if seen.insert(next.join("::")) && seen.len() <= 64 {
                if g.site_public {
                    public_chains.push(next.clone());
                }
                queue.push(next);
            }
        }
        if let Some(soft) = soft {
            for b in soft {
                let attaches = match &b.source_parent {
                    Some(p) => {
                        chain.last() == Some(p)
                            && (!b.site_relative || chain.starts_with(&b.chain))
                    }
                    None => no_parent_attaches,
                };
                if !attaches {
                    continue;
                }
                if seen.insert(b.chain.join("::")) && seen.len() <= 64 {
                    if b.site_public {
                        public_chains.push(b.chain.clone());
                    }
                    queue.push(b.chain.clone());
                }
            }
        }
    }
    public_chains
}

/// What: mint pair-shaped decl keys (zero counts) for one leg's
/// candidates: an existing usage-driven spelling wins the canonical
/// outer, else the shortest public chain; alternate outers + the
/// hard parent ride `pair_aliases`. NF5 parity holds (a minted key
/// must not resurrect a substring-dropped family).
#[allow(clippy::too_many_arguments)]
fn mint_pair_decls(
    decls: &[DeclCandidate],
    name_counts: &HashMap<(String, String), usize>,
    bindings: &HashMap<(String, String), Vec<SoftBinding>>,
    glob_bindings: &[GlobBinding],
    mods: &ModIndex,
    crates: &indexmap::IndexMap<String, CrateInfo>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
    pair_aliases: &mut BTreeMap<String, std::collections::BTreeSet<String>>,
    make_pattern: &dyn Fn(&str, &str) -> Pattern,
) {
    for decl in decls {
        let public_chains =
            public_chains_for(decl, name_counts, bindings, glob_bindings, mods);
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
            .find(|o| metrics.contains_key(&make_pattern(o, &decl.name).to_string()));
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
        let key = make_pattern(&canonical_outer, &decl.name);
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
}

/// What: mint bare-name decl keys (structure:/traits:, zero counts)
/// for pub-reachable module-level type/trait decls no usage stream
/// keyed - the type-side DECLARED-UNCAPTURED class (bevy's
/// `pub type Write`, `Disabled`, `Mutable`). Weighted-render-only:
/// minted keys stay unrendered until the consumer-weight term
/// scores them.
#[allow(clippy::too_many_arguments)]
fn mint_type_decls(
    rows: &[serde_json::Value],
    crates: &indexmap::IndexMap<String, CrateInfo>,
    mods: &ModIndex,
    bindings: &HashMap<(String, String), Vec<SoftBinding>>,
    glob_bindings: &[GlobBinding],
    name_counts: &HashMap<(String, String), usize>,
    metrics: &mut indexmap::IndexMap<String, PatternMetric>,
    calibration: &Calibration,
    make_pattern: &dyn Fn(&str) -> Pattern,
) {
    let decls = collect_decl_rows(rows, crates, mods, false);
    for decl in &decls {
        let key_wire = make_pattern(&decl.name).to_string();
        if metrics.contains_key(&key_wire) {
            continue;
        }
        if should_skip_pattern(&key_wire, calibration) {
            continue;
        }
        let reachable =
            !public_chains_for(decl, name_counts, bindings, glob_bindings, mods).is_empty();
        if !reachable {
            continue;
        }
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
