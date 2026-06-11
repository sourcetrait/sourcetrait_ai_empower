use crate::*;

/// What: the six-set significance picker (architecture / public /
/// inter_crate / clique / intra_crate / inner_crate) computing per-
/// pattern scores and STV-elected clique. Returns the SignificanceSets
/// struct consumed by section renderers.
///
/// Why: emit.py's `_compute_significance_sets` (lines 646-935, ~290
/// lines). The architectural-protagonist picks come out of this. STV
/// (single transferable vote) clique replaces the prior 'internals'
/// heuristic so each crate has equal voice rather than being dominated
/// by one heavy-user crate.
///
/// Where: called from `crate::emit::run::emit` after fingerprint +
/// facts are loaded; output threaded into section renderers (S5 +
/// downstream picker-driven sections).
/// What: post-expansion public trait/type name sets from the rustdoc
/// overlay, used to backfill `is_pub` for entries whose declaration
/// visibility the floor's token walk could not see.
///
/// Why: bevy's define_label! emits `pub trait ScheduleLabel` from a
/// name-only invocation - the floor records the trait with blank
/// visibility and every public-set eligibility dies on is_pub. The
/// overlay sees the expansion; overlay-bearing emits flip the flag.
///
/// Where: built by `emit::run::emit` from rustdoc_overlay.json;
/// consulted in the public-scores loop of
/// `compute_significance_sets`.
pub struct VisBackfill {
    pub traits: HashSet<String>,
    pub types: HashSet<String>,
    /// The overlay's documented package: the defining-crate fallback
    /// for entries the floor could not attribute at all (fully
    /// macro-generated items have no declaration fact; rustdoc
    /// attests both visibility and the owning package).
    pub package: Option<String>,
}

impl VisBackfill {
    /// What: true when the overlay's pub sets vouch for this
    /// pattern's declaration being public (group-aware: traits /
    /// configuring check the trait set; structure checks the type
    /// set).
    pub fn backfills(&self, pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Traits(n) | Pattern::Configuring(n) => self.traits.contains(n),
            Pattern::Structure(n) => self.types.contains(n),
            _ => false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn compute_significance_sets(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
    per_crate_sloc: &indexmap::IndexMap<String, usize>,
    top_n_workspace: usize,
    calibration: &Calibration,
    weights: Option<&TargetWeights>,
    profile: &ProfileSetScale,
    vis_backfill: Option<&VisBackfill>,
    adopted_credit: &HashMap<String, Vec<String>>,
) -> SignificanceSets {
    let pattern_metrics = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // Parse the fingerprint's string-keyed pattern_metrics into the typed
    // grammar once - the picker's only string-parse boundary; everything
    // downstream threads `Pattern`.
    let pm_by_pattern: indexmap::IndexMap<Pattern, &serde_json::Value> = pattern_metrics
        .iter()
        .filter_map(|(k, v)| Pattern::from_wire(k).map(|p| (p, v)))
        .collect();
    // R8 slice 5: scaffolding-DEFINED patterns are pick-ineligible
    // (demo-app types reaching public/arch, R7 topic m). Usage FROM
    // scaffolding crates still counts - the gate is origin-side only.
    let scaffolding: HashSet<String> = fp
        .get("workspace_use_classification")
        .and_then(|v| v.get("scaffolding_crates"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let is_workspace_originated = |pat: &Pattern| -> bool {
        pm_by_pattern
            .get(pat)
            .and_then(|m| m.get("defining_crate"))
            .and_then(|v| v.as_str())
            .map(|c| !scaffolding.contains(c))
            .unwrap_or(false)
    };

    // Per-crate ballot resolution gate (the_user ruling, dev block
    // 2): ballots mirror the key-level counting loop's PER-SITE
    // resolution so same-named std/foreign usage never sweeps into
    // workspace keys (the nushell structure:File / tokio
    // SocketAddr::V4 class). Third consumer of the shared
    // resolution substrate; the vocabulary rebuilds from the
    // fingerprint's per-crate identity fields (package / lib_name /
    // renames - the renames ride the wire for exactly this).
    let empty_rows: Vec<serde_json::Value> = Vec::new();
    let uses_arr = facts.get("uses").and_then(|v| v.as_array()).unwrap_or(&empty_rows);
    let import_maps = build_import_bindings(uses_arr.iter().filter_map(|u| {
        Some((
            u.get("file").and_then(|v| v.as_str())?,
            u.get("path").and_then(|v| v.as_str())?,
        ))
    }));
    let vocab_crates: indexmap::IndexMap<String, CrateInfo> = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let lib_name = v
                        .get("lib_name")
                        .and_then(|x| x.as_str())
                        .map(String::from);
                    let renames: Vec<(String, String)> = v
                        .get("renames")
                        .and_then(|x| x.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|p| {
                                    let pair = p.as_array()?;
                                    Some((
                                        pair.first()?.as_str()?.to_string(),
                                        pair.get(1)?.as_str()?.to_string(),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (
                        k.clone(),
                        CrateInfo {
                            lib_name,
                            renames,
                            ..Default::default()
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let vocab = ResolveVocab::from_crates(&vocab_crates);
    let mut local_decl_crates: HashMap<String, HashSet<String>> = HashMap::new();
    for list_key in ["types", "traits"] {
        if let Some(arr) = facts.get(list_key).and_then(|v| v.as_array()) {
            for t in arr {
                if is_example_path(t.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                    continue;
                }
                if let (Some(n), Some(c)) = (
                    t.get("name").and_then(|v| v.as_str()),
                    t.get("crate").and_then(|v| v.as_str()),
                ) {
                    local_decl_crates
                        .entry(n.to_string())
                        .or_default()
                        .insert(c.to_string());
                }
            }
        }
    }
    let mut local_mod_crates: HashMap<String, HashSet<String>> = HashMap::new();
    if let Some(arr) = facts.get("mods").and_then(|v| v.as_array()) {
        for m in arr {
            if is_example_path(m.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                continue;
            }
            if let (Some(n), Some(c)) = (
                m.get("name").and_then(|v| v.as_str()),
                m.get("crate").and_then(|v| v.as_str()),
            ) {
                local_mod_crates
                    .entry(n.to_string())
                    .or_default()
                    .insert(c.to_string());
            }
        }
    }
    let facades = build_facade_index(
        uses_arr.iter().filter_map(|u| {
            if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
                return None;
            }
            if is_example_path(u.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
                return None;
            }
            Some((
                u.get("crate").and_then(|v| v.as_str())?,
                u.get("path").and_then(|v| v.as_str())?,
            ))
        }),
        &vocab,
    );
    let ballot_credits = |pat: &Pattern,
                          resolve_name: &str,
                          qualifier: Option<&str>,
                          file: &str,
                          using: &str|
     -> bool {
        let m = match pm_by_pattern.get(pat) {
            Some(m) => m,
            None => return false,
        };
        let defining = match m.get("defining_crate").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return false,
        };
        if m.get("adopted").and_then(|v| v.as_str()).is_some() {
            // Adopted-root rule (mirrors count_adopted_usage): the
            // site's written qualifier or import binding resolves to
            // the root binding or an adopting crate.
            let adopters = adopted_credit.get(defining);
            let hit = |r: &str| -> bool {
                let rn = r.replace('-', "_");
                rn == defining
                    || adopters
                        .map(|a| a.iter().any(|c| c.replace('-', "_") == rn))
                        .unwrap_or(false)
            };
            if let Some(q) = qualifier {
                return hit(q);
            }
            if let Some(b) = import_maps.get(file).and_then(|mm| mm.get(resolve_name)) {
                return hit(&b.root);
            }
            return false;
        }
        let origin = resolve_site_origin(
            file,
            resolve_name,
            qualifier,
            using,
            &import_maps,
            &vocab,
            &local_decl_crates,
            &local_mod_crates,
        );
        site_credits(&origin, using, defining, resolve_name, &facades)
    };

    // R3: per-crate pre-aggregation uses the picks-data model's
    // `<group>:<name>` pattern key shape (see
    // `notes/know_rust/working/02_picks_data.md`). Mapping:
    //   impls (trait T)        -> traits:T
    //   derives (trait T)      -> configuring:T
    //   type_usages (O::i)     -> BRIDGE: structure:O AND
    //                             implementation_functions:O::i
    //   macros: attr_macro M   -> configuring:M
    //           reg_macro M    -> utilities:M
    let mut per_crate_counts: indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>> =
        indexmap::IndexMap::new();
    if let Some(arr) = facts.get("impls").and_then(|v| v.as_array()) {
        for it in arr {
            if let Some(trait_name) = it.get("trait").and_then(|v| v.as_str()) {
                if !it.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false) {
                    if let Some(c) = it.get("crate").and_then(|v| v.as_str()) {
                        let p = Pattern::traits(trait_name);
                        let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("");
                        if is_workspace_originated(&p)
                            && ballot_credits(&p, trait_name, None, file, c)
                        {
                            *per_crate_counts
                                .entry(c.to_string())
                                .or_default()
                                .entry(p)
                                .or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }
    if let Some(arr) = facts.get("derives").and_then(|v| v.as_array()) {
        for d in arr {
            if let (Some(c), Some(nm)) = (
                d.get("crate").and_then(|v| v.as_str()),
                d.get("trait").and_then(|v| v.as_str()),
            ) {
                let p = Pattern::configuring(nm);
                let file = d.get("file").and_then(|v| v.as_str()).unwrap_or("");
                if is_workspace_originated(&p) && ballot_credits(&p, nm, None, file, c) {
                    *per_crate_counts
                        .entry(c.to_string())
                        .or_default()
                        .entry(p)
                        .or_insert(0) += 1;
                }
            }
        }
    }
    if let Some(arr) = facts.get("type_usages").and_then(|v| v.as_array()) {
        for tu in arr {
            if let (Some(c), Some(nm)) = (
                tu.get("crate").and_then(|v| v.as_str()),
                tu.get("name").and_then(|v| v.as_str()),
            ) {
                // BRIDGE: each O::i type_usage contributes to BOTH
                // structure:O (the type's architectural footprint) and
                // implementation_functions:O::i (the per-method usage).
                // The gate resolves the OUTER, mirroring the counting
                // loop's resolve_target for type_usage sites.
                let file = tu.get("file").and_then(|v| v.as_str()).unwrap_or("");
                let qualifier = tu.get("qualifier").and_then(|v| v.as_str());
                let outer = nm.split_once("::").map(|(o, _)| o).unwrap_or(nm);
                let impl_fn = Pattern::from_group_name(PickGroup::ImplementationFunctions, nm);
                if is_workspace_originated(&impl_fn)
                    && ballot_credits(&impl_fn, outer, qualifier, file, c)
                {
                    *per_crate_counts
                        .entry(c.to_string())
                        .or_default()
                        .entry(impl_fn)
                        .or_insert(0) += 1;
                }
                if let Some(outer) = nm.split_once("::").map(|(o, _)| o) {
                    let struct_pat = Pattern::structure(outer);
                    if is_workspace_originated(&struct_pat)
                        && ballot_credits(&struct_pat, outer, qualifier, file, c)
                    {
                        *per_crate_counts
                            .entry(c.to_string())
                            .or_default()
                            .entry(struct_pat)
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }
    if let Some(arr) = facts.get("macros").and_then(|v| v.as_array()) {
        for m in arr {
            let c = m.get("crate").and_then(|v| v.as_str());
            let kind = m.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let nm = m.get("name").and_then(|v| v.as_str());
            if let (Some(c), Some(nm)) = (c, nm) {
                // attr_macro -> configuring (attribute-driven compile-
                // time integration); reg_macro (macro_invocation) ->
                // utilities (an invocation call site, not attribute
                // integration). The picks-data model splits the two.
                let p = match kind {
                    "attr_macro" => Some(Pattern::configuring(nm)),
                    "macro_invocation" => Some(Pattern::utilities(nm)),
                    _ => None,
                };
                if let Some(p) = p {
                    let file = m.get("file").and_then(|v| v.as_str()).unwrap_or("");
                    if is_workspace_originated(&p) && ballot_credits(&p, nm, None, file, c) {
                        *per_crate_counts
                            .entry(c.to_string())
                            .or_default()
                            .entry(p)
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let num_example_rs_files = fp
        .get("totals")
        .and_then(|t| t.get("example_rs_files"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let public_example_weight =
        compute_public_example_weight(num_example_rs_files, calibration);

    let mut public_scores: indexmap::IndexMap<Pattern, f64> = indexmap::IndexMap::new();
    let mut inter_scores: indexmap::IndexMap<Pattern, usize> = indexmap::IndexMap::new();
    for (pattern, m) in &pm_by_pattern {
        // Defining-crate, with the overlay package as the fallback
        // for vis-backfilled entries the floor could not attribute
        // (fully macro-generated declarations leave no fact).
        let backfilled = vis_backfill.map(|v| v.backfills(pattern)).unwrap_or(false);
        let dc = match m.get("defining_crate").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => match (
                backfilled,
                vis_backfill.and_then(|v| v.package.as_deref()),
            ) {
                (true, Some(p)) => p,
                _ => continue,
            },
        };
        if scaffolding.contains(dc) {
            continue;
        }
        let ic = m.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let curated = m
            .get("curated_example_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let is_pub =
            m.get("is_pub").and_then(|v| v.as_bool()).unwrap_or(false) || backfilled;
        if is_pub && curated > 0 {
            public_scores.insert(pattern.clone(), curated as f64 * public_example_weight);
        }
        if ic > 0 {
            inter_scores.insert(pattern.clone(), ic);
        }
    }

    // Consumer-demand weight term: revealed third-party demand from
    // the weight blob. Zero-usage decl-channel pair keys earn their
    // BASE public score from demand sites (public-by-consumption);
    // usage-backed keys already in the public pool get an additive
    // per-site boost. Pair spellings resolve through facts'
    // pair_aliases so a demand recorded under an alternate binding
    // outer still lands on the one rendered key.
    if let Some(tw) = weights {
        let cw = &calibration.picker.consumer_weight;
        let alias_outers: HashMap<String, Vec<String>> = facts
            .get("pair_aliases")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|o| o.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Pair-site lookup shared by the pair-shaped weight arms:
        // the rendered spelling plus every pair-alias spelling.
        let pair_sites = |outer: &str, inner: &str| -> usize {
            let pair = format!("{}::{}", outer, inner);
            let mut sites = tw.pairs.get(&pair).map(|c| c.sites).unwrap_or(0);
            if let Some(aliases) = alias_outers.get(&pair) {
                for a in aliases {
                    sites += tw
                        .pairs
                        .get(&format!("{}::{}", a, inner))
                        .map(|c| c.sites)
                        .unwrap_or(0);
                }
            }
            sites
        };
        let name_sites = |n: &str| -> usize { tw.names.get(n).map(|c| c.sites).unwrap_or(0) };
        for (pattern, m) in &pm_by_pattern {
            let dc = m.get("defining_crate").and_then(|v| v.as_str());
            if dc.map(|c| scaffolding.contains(c)).unwrap_or(true) {
                continue;
            }
            // The public pool's own bar holds for demand entries:
            // a non-pub key never scores public-by-consumption
            // (blob names fold miss records, which can carry
            // collision noise).
            let backfilled = vis_backfill.map(|v| v.backfills(pattern)).unwrap_or(false);
            if !(m.get("is_pub").and_then(|v| v.as_bool()).unwrap_or(false) || backfilled) {
                continue;
            }
            let usage_total = m.get("intra_count").and_then(|v| v.as_u64()).unwrap_or(0)
                + m.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let sites = match pattern {
                // Pair keys (fn + const) consult pair demand under
                // the rendered spelling + aliases; zero-usage decl
                // mints additionally consult the inner's NAME cell -
                // import-shaped demand (`use nu_protocol::
                // NU_VARIABLE_ID;`, palette imports) records a name,
                // not a pair, and the decl key is that demand's only
                // server. Name-level inner attribution is the
                // system's standing granularity trade.
                Pattern::ImplementationFunctions { outer, inner } if outer != "_" => {
                    pair_sites(outer, inner)
                        + if usage_total == 0 { name_sites(inner) } else { 0 }
                }
                Pattern::Globals(name) => match name.split_once("::") {
                    Some((o, i)) => {
                        pair_sites(o, i)
                            + if usage_total == 0 { name_sites(i) } else { 0 }
                    }
                    None => continue,
                },
                // Bare type/trait keys: the name demand IS the
                // demand (the bevy `pub type Write` + `Disabled`
                // classes).
                Pattern::Structure(n) | Pattern::Traits(n) => name_sites(n),
                _ => continue,
            };
            if sites == 0 {
                continue;
            }
            // Public-by-consumption, uniformly: a demanded key
            // ABSENT from the public pool earns the demand BASE
            // (internal usage does not disqualify consumer-facing
            // significance - the avian `Disabled` class: 5 internal
            // sites, 73 consumer demand sites, no curated evidence);
            // a key already public-scored earns the additive boost.
            match public_scores.get_mut(pattern) {
                Some(s) => *s += cw.usage_boost * sites as f64,
                None => {
                    public_scores.insert(pattern.clone(), cw.site_weight * sites as f64);
                }
            }
        }
    }

    let architecture_keys: indexmap::IndexSet<Pattern> = public_scores
        .keys()
        .filter(|k| inter_scores.contains_key(*k))
        .cloned()
        .collect();
    let mut architecture_counts: indexmap::IndexMap<Pattern, f64> = indexmap::IndexMap::new();
    for p in &architecture_keys {
        let total = public_scores.get(p).copied().unwrap_or(0.0)
            + inter_scores.get(p).copied().unwrap_or(0) as f64;
        architecture_counts.insert(p.clone(), total);
    }

    let cap_matrix = &calibration.picker.cap_matrix;
    let floor = calibration.picker.top_n_floor;

    // R8 slice 7 (the_user): dedup provides UNIQUE data, it never
    // LOSES data. Subtraction runs against RENDERED (capped) sets in
    // precedence order 5.1 > 5.2 > 5.3 - a candidate cut by one set's
    // cap falls to its next qualifying set with that set's own score;
    // only cap competition may drop a pick. (The prior candidate-level
    // subtraction made an arch-cap-cut candidate vanish from the whole
    // workspace-wide tier.)
    let significant_architecture = bucket_and_cap_by_group(
        &architecture_counts,
        PickSet::Architecture,
        top_n_workspace,
        cap_matrix,
        floor,
        profile,
    );
    let public_counts: indexmap::IndexMap<Pattern, f64> = public_scores
        .iter()
        .filter(|(k, _)| !significant_architecture.contains_key(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let significant_public = bucket_and_cap_by_group(
        &public_counts,
        PickSet::Public,
        top_n_workspace,
        cap_matrix,
        floor,
        profile,
    );
    let inter_counts: indexmap::IndexMap<Pattern, usize> = inter_scores
        .iter()
        .filter(|(k, _)| {
            !significant_architecture.contains_key(*k) && !significant_public.contains_key(*k)
        })
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let significant_inter_crate = bucket_and_cap_by_group(
        &inter_counts,
        PickSet::InterCrate,
        top_n_workspace,
        cap_matrix,
        floor,
        profile,
    );

    let mut workspace_wide_keys: indexmap::IndexSet<Pattern> = indexmap::IndexSet::new();
    for k in significant_architecture.keys() {
        workspace_wide_keys.insert(k.clone());
    }
    for k in significant_inter_crate.keys() {
        workspace_wide_keys.insert(k.clone());
    }
    for k in significant_public.keys() {
        workspace_wide_keys.insert(k.clone());
    }

    // R8 slice 4 (the_user-confirmed): the STV election sees the FULL
    // per-crate intra pools - crate-equal voice must not be starved by
    // workspace-wide widening (R7 topic d emptied clique/intra on five
    // targets). The elected list renders as-is; duplication with the
    // workspace-wide sets is a different lens, kept. Workspace-wide
    // dedup applies to the rendered 5.5 lists only (below).
    // The ELECTION sees full pools regardless of profile (the R8
    // crate-equal-voice ruling): ballots build at NEUTRAL scale; the
    // profile applies to the SEAT count and the rendered sets below.
    let neutral = ProfileSetScale::neutral();
    let empty_dedup: indexmap::IndexSet<Pattern> = indexmap::IndexSet::new();
    let clique_scale = profile.for_set(PickSet::Clique);
    let elected_clique = if clique_scale <= 0.0 {
        indexmap::IndexMap::new()
    } else {
        let ballots_intra_per_crate = per_crate_picks(
            &per_crate_counts,
            &pattern_metrics,
            per_crate_sloc,
            calibration,
            PickSet::IntraCrate,
            false,
            &empty_dedup,
            &neutral,
        )
        .0;
        let per_crate_ballots: indexmap::IndexMap<String, Vec<Pattern>> = ballots_intra_per_crate
            .iter()
            .map(|(c, s)| (c.clone(), s.keys().cloned().collect()))
            .collect();
        // R4b: clique seats = workspace base * Clique set_mult (*
        // profile scale). Run STV for this many seats, then
        // post-filter via per-group caps. The post-filter only trims
        // groups whose elected count exceeds the group's cap; with
        // current weights, most per-group caps exceed the STV seat
        // count (e.g. clique_seats=44 at base=29 vs traits cap=65) so
        // the filter is a defensive ceiling rather than a routine
        // trim.
        let clique_seats = {
            let set_mult = cap_matrix.set.for_set(PickSet::Clique);
            ((top_n_workspace as f64 * set_mult * clique_scale).round() as usize).max(floor)
        };
        stv_elect_clique(&per_crate_ballots, clique_seats, &empty_dedup)
    };
    // the_user (R8 slice 4 correction): the clique END RESULT stays
    // deduped against the workspace-wide sets - only the ELECTION
    // sees full pools. Winners that already sit in arch / public /
    // inter are dropped from the rendered list; surplus-transfer
    // winners below them remain.
    let elected_deduped: indexmap::IndexMap<Pattern, f64> = elected_clique
        .into_iter()
        .filter(|(k, _)| !workspace_wide_keys.contains(k))
        .collect();
    let significant_clique = bucket_and_cap_by_group(
        &elected_deduped,
        PickSet::Clique,
        top_n_workspace,
        cap_matrix,
        floor,
        profile,
    );

    for k in significant_clique.keys() {
        workspace_wide_keys.insert(k.clone());
    }

    let (significant_intra_crate_per_crate, top_n_intra_crate_per_crate) = per_crate_picks(
        &per_crate_counts,
        &pattern_metrics,
        per_crate_sloc,
        calibration,
        PickSet::IntraCrate,
        false,
        &workspace_wide_keys,
        profile,
    );
    let (significant_inner_crate_per_crate, top_n_inner_per_crate) = per_crate_picks(
        &per_crate_counts,
        &pattern_metrics,
        per_crate_sloc,
        calibration,
        PickSet::InnerCrate,
        true,
        &workspace_wide_keys,
        profile,
    );

    SignificanceSets {
        significant_intra_crate_per_crate,
        significant_inner_crate_per_crate,
        significant_inter_crate,
        significant_public,
        significant_architecture,
        significant_clique,
        top_n_intra_crate_per_crate,
        top_n_inner_per_crate,
    }
}

fn per_crate_picks(
    per_crate_counts: &indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>>,
    pattern_metrics: &serde_json::Map<String, serde_json::Value>,
    per_crate_sloc: &indexmap::IndexMap<String, usize>,
    calibration: &Calibration,
    set: PickSet,
    origin_match: bool,
    dedup_keys: &indexmap::IndexSet<Pattern>,
    profile: &ProfileSetScale,
) -> (
    indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>>,
    indexmap::IndexMap<String, usize>,
) {
    let mut sig_per_crate: indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>> =
        indexmap::IndexMap::new();
    let mut top_n_per_crate: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    if profile.for_set(set) <= 0.0 {
        return (sig_per_crate, top_n_per_crate);
    }
    let cap_matrix = &calibration.picker.cap_matrix;
    let floor = calibration.picker.top_n_floor;
    for (crate_name, counts) in per_crate_counts {
        if counts.is_empty() {
            continue;
        }
        let base_cap = compute_sloc_scaled_top_n(
            per_crate_sloc.get(crate_name).copied().unwrap_or(0),
            calibration,
        );
        top_n_per_crate.insert(crate_name.clone(), base_cap);
        let mut filtered: indexmap::IndexMap<Pattern, usize> = indexmap::IndexMap::new();
        for (p, c) in counts {
            if dedup_keys.contains(p) {
                continue;
            }
            let defining = pattern_metrics
                .get(&p.to_string())
                .and_then(|m| m.get("defining_crate"))
                .and_then(|v| v.as_str())
                .map(String::from);
            if origin_match && defining.as_deref() != Some(crate_name.as_str()) {
                continue;
            }
            if !origin_match
                && (defining.is_none() || defining.as_deref() == Some(crate_name.as_str()))
            {
                continue;
            }
            filtered.insert(p.clone(), *c);
        }
        if filtered.is_empty() {
            continue;
        }
        let sig = bucket_and_cap_by_group(&filtered, set, base_cap, cap_matrix, floor, profile);
        if !sig.is_empty() {
            sig_per_crate.insert(crate_name.clone(), sig);
        }
    }
    (sig_per_crate, top_n_per_crate)
}

/// What: bucket the input score map by `PickGroup` (extracted from
/// each pattern key's `<group_wire>:<name>` prefix) and apply the R4b
/// cap matrix per (group, set). Returns the union of per-group top-N
/// entries.
///
/// Why: R4b replaces the single global top-N cap with per-(group, set)
/// cell-specific caps via the cap matrix. Workspace-wide sets
/// (Architecture / Public / InterCrate) and per-crate sets
/// (IntraCrate / InnerCrate) consume this helper after computing
/// their score per pattern. Clique uses it for its post-STV cap.
///
/// Patterns whose key prefix is not a recognized `PickGroup` wire
/// token are dropped (defense in depth; the picker only emits
/// recognized group keys post-R3 translation).
///
/// Where: called from `compute_significance_sets` (workspace-wide
/// sets + post-STV clique filter) and `per_crate_picks` (per-crate
/// sets). The documentation-kind profile scales the matrix cap per
/// set: scale 0 turns the set OFF (empty result); a nonzero scale
/// multiplies the cell cap before the floor applies.
fn bucket_and_cap_by_group<V>(
    counts: &indexmap::IndexMap<Pattern, V>,
    set: PickSet,
    base_cap: usize,
    matrix: &CapMatrix,
    floor: usize,
    profile: &ProfileSetScale,
) -> indexmap::IndexMap<Pattern, V>
where
    V: Clone + PartialOrd,
{
    let scale = profile.for_set(set);
    if scale <= 0.0 {
        return indexmap::IndexMap::new();
    }
    let mut by_group: HashMap<PickGroup, Vec<(Pattern, V)>> = HashMap::new();
    for (k, v) in counts {
        by_group
            .entry(k.kind())
            .or_default()
            .push((k.clone(), v.clone()));
    }
    // Determinism: HashMap iteration order is per-process random, and
    // an unstable sort lets cap-boundary ties be cut arbitrarily -
    // which randomized both pick membership at the boundary and the
    // STV ballot ranking built from this map (clique elections
    // differed between identical-code runs). Sort with a full
    // tie-break (score desc, then pattern key) inside each group AND
    // across the returned map, so output is byte-reproducible per
    // the sampling contract.
    let mut capped: Vec<(Pattern, V)> = Vec::new();
    for (group, mut items) in by_group {
        let cap = if (scale - 1.0).abs() < f64::EPSILON {
            matrix.cap_for(group, set, base_cap, floor)
        } else {
            let scaled = (base_cap as f64)
                * matrix.group.for_group(group)
                * matrix.set.for_set(set)
                * scale;
            (scaled.round() as usize).max(floor)
        };
        items.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        capped.extend(items.into_iter().take(cap));
    }
    capped.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    capped.into_iter().collect()
}

/// What: SLOC-scaled top-N cap formula. cap = max(floor, round(floor +
/// multiplier * log2(sloc / divisor))).
pub fn compute_sloc_scaled_top_n(sloc: usize, calibration: &Calibration) -> usize {
    let floor = calibration.picker.top_n_floor;
    if sloc == 0 {
        return floor;
    }
    let divisor = calibration.picker.sloc_divisor as f64;
    let multiplier = calibration.picker.sloc_multiplier;
    let ratio = sloc as f64 / divisor;
    if ratio < 1.0 {
        return floor;
    }
    let scaled = floor as f64 + multiplier * ratio.log2();
    let rounded = format!("{:.0}", scaled).parse::<usize>().unwrap_or(floor);
    rounded.max(floor)
}

/// What: log-scaled per-example weight for the public set.
pub fn compute_public_example_weight(
    num_example_rs_files: usize,
    calibration: &Calibration,
) -> f64 {
    let floor = calibration.picker.example.weight_floor;
    if num_example_rs_files <= 1 {
        return floor;
    }
    floor.max((num_example_rs_files as f64).log2())
}

/// What: Single Transferable Vote (STV) election with fractional Droop
/// quota. Each crate is a voter, ballot is its intra top-N. Returns
/// dict of {pattern: vote_total} in election order.
pub fn stv_elect_clique(
    per_crate_ballots: &indexmap::IndexMap<String, Vec<Pattern>>,
    num_seats: usize,
    dedup_keys: &indexmap::IndexSet<Pattern>,
) -> indexmap::IndexMap<Pattern, f64> {
    let mut ballots: Vec<Vec<Pattern>> = Vec::new();
    for ranked in per_crate_ballots.values() {
        let clean: Vec<Pattern> = ranked
            .iter()
            .filter(|p| !dedup_keys.contains(*p))
            .cloned()
            .collect();
        if !clean.is_empty() {
            ballots.push(clean);
        }
    }
    let v_count = ballots.len();
    let k = num_seats;
    if v_count == 0 || k == 0 {
        return indexmap::IndexMap::new();
    }
    let q = v_count as f64 / (k as f64 + 1.0);
    let mut weights: Vec<f64> = vec![1.0; v_count];
    let mut pointers: Vec<usize> = vec![0; v_count];
    let mut elected: indexmap::IndexMap<Pattern, f64> = indexmap::IndexMap::new();
    let mut eliminated: indexmap::IndexSet<Pattern> = indexmap::IndexSet::new();

    let current = |i: usize, pointers: &mut Vec<usize>, elected: &indexmap::IndexMap<Pattern, f64>, eliminated: &indexmap::IndexSet<Pattern>, ballots: &Vec<Vec<Pattern>>| -> Option<Pattern> {
        while pointers[i] < ballots[i].len() {
            let p = &ballots[i][pointers[i]];
            if elected.contains_key(p) || eliminated.contains(p) {
                pointers[i] += 1;
            } else {
                return Some(p.clone());
            }
        }
        None
    };

    while elected.len() < k {
        let mut tally: indexmap::IndexMap<Pattern, f64> = indexmap::IndexMap::new();
        let mut supporters: indexmap::IndexMap<Pattern, Vec<usize>> = indexmap::IndexMap::new();
        for i in 0..v_count {
            if weights[i] <= 0.0 {
                continue;
            }
            if let Some(p) = current(i, &mut pointers, &elected, &eliminated, &ballots) {
                *tally.entry(p.clone()).or_insert(0.0) += weights[i];
                supporters.entry(p).or_default().push(i);
            }
        }
        if tally.is_empty() {
            break;
        }
        let mut over_quota: Vec<(Pattern, f64)> = tally
            .iter()
            .filter(|item| *item.1 >= q)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        if !over_quota.is_empty() {
            over_quota.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            });
            for (c, votes) in over_quota {
                if elected.len() >= k {
                    break;
                }
                elected.insert(c.clone(), votes);
                let surplus = votes - q;
                let transfer_factor = if votes > 0.0 { surplus / votes } else { 0.0 };
                if let Some(sup) = supporters.get(&c) {
                    for &i in sup {
                        weights[i] *= transfer_factor;
                    }
                }
            }
        } else {
            let mut tally_vec: Vec<(Pattern, f64)> = tally
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            tally_vec.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            });
            if let Some((min_pattern, _)) = tally_vec.first() {
                eliminated.insert(min_pattern.clone());
            }
        }
    }
    elected
}

/// What: full output of compute_significance_sets - the six sets plus
/// per-crate top-N caps.
#[derive(Debug, Clone)]
pub struct SignificanceSets {
    pub significant_intra_crate_per_crate:
        indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>>,
    pub significant_inner_crate_per_crate:
        indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>>,
    pub significant_inter_crate: indexmap::IndexMap<Pattern, usize>,
    pub significant_public: indexmap::IndexMap<Pattern, f64>,
    pub significant_architecture: indexmap::IndexMap<Pattern, f64>,
    pub significant_clique: indexmap::IndexMap<Pattern, f64>,
    pub top_n_intra_crate_per_crate: indexmap::IndexMap<String, usize>,
    pub top_n_inner_per_crate: indexmap::IndexMap<String, usize>,
}
