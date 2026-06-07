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
pub fn compute_significance_sets(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
    per_crate_sloc: &indexmap::IndexMap<String, usize>,
    top_n_workspace: usize,
    calibration: &Calibration,
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
    let is_workspace_originated = |pat: &Pattern| -> bool {
        pm_by_pattern
            .get(pat)
            .and_then(|m| m.get("defining_crate"))
            .map(|v| !v.is_null())
            .unwrap_or(false)
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
                        if is_workspace_originated(&p) {
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
                if is_workspace_originated(&p) {
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
                let impl_fn = Pattern::from_group_name(PickGroup::ImplementationFunctions, nm);
                if is_workspace_originated(&impl_fn) {
                    *per_crate_counts
                        .entry(c.to_string())
                        .or_default()
                        .entry(impl_fn)
                        .or_insert(0) += 1;
                }
                if let Some(outer) = nm.split_once("::").map(|(o, _)| o) {
                    let struct_pat = Pattern::structure(outer);
                    if is_workspace_originated(&struct_pat) {
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
                    if is_workspace_originated(&p) {
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
        if m.get("defining_crate").map(|v| v.is_null()).unwrap_or(true) {
            continue;
        }
        let ic = m.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let curated = m
            .get("curated_example_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let is_pub = m.get("is_pub").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_pub && curated > 0 {
            public_scores.insert(pattern.clone(), curated as f64 * public_example_weight);
        }
        if ic > 0 {
            inter_scores.insert(pattern.clone(), ic);
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
    let inter_counts: indexmap::IndexMap<Pattern, usize> = inter_scores
        .iter()
        .filter(|(k, _)| !architecture_keys.contains(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let public_counts: indexmap::IndexMap<Pattern, f64> = public_scores
        .iter()
        .filter(|(k, _)| !architecture_keys.contains(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect();

    let cap_matrix = &calibration.picker.cap_matrix;
    let floor = calibration.picker.top_n_floor;

    let significant_inter_crate = bucket_and_cap_by_group(
        &inter_counts,
        PickSet::InterCrate,
        top_n_workspace,
        cap_matrix,
        floor,
    );
    let significant_public = bucket_and_cap_by_group(
        &public_counts,
        PickSet::Public,
        top_n_workspace,
        cap_matrix,
        floor,
    );
    let significant_architecture = bucket_and_cap_by_group(
        &architecture_counts,
        PickSet::Architecture,
        top_n_workspace,
        cap_matrix,
        floor,
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

    let initial_intra_per_crate = per_crate_picks(
        &per_crate_counts,
        &pattern_metrics,
        per_crate_sloc,
        calibration,
        PickSet::IntraCrate,
        false,
        &workspace_wide_keys,
    )
    .0;
    let per_crate_ballots: indexmap::IndexMap<String, Vec<Pattern>> = initial_intra_per_crate
        .iter()
        .map(|(c, s)| (c.clone(), s.keys().cloned().collect()))
        .collect();
    // R4b: clique seats = workspace base * Clique set_mult. Run STV
    // for this many seats, then post-filter via per-group caps. The
    // post-filter only trims groups whose elected count exceeds the
    // group's cap; with current weights, most per-group caps exceed
    // the STV seat count (e.g. clique_seats=44 at base=29 vs traits
    // cap=65) so the filter is a defensive ceiling rather than a
    // routine trim.
    let clique_seats = {
        let set_mult = cap_matrix.set.for_set(PickSet::Clique);
        ((top_n_workspace as f64 * set_mult).round() as usize).max(floor)
    };
    let elected_clique = stv_elect_clique(
        &per_crate_ballots,
        clique_seats,
        &workspace_wide_keys,
    );
    let significant_clique = bucket_and_cap_by_group(
        &elected_clique,
        PickSet::Clique,
        top_n_workspace,
        cap_matrix,
        floor,
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
    );
    let (significant_inner_crate_per_crate, top_n_inner_per_crate) = per_crate_picks(
        &per_crate_counts,
        &pattern_metrics,
        per_crate_sloc,
        calibration,
        PickSet::InnerCrate,
        true,
        &workspace_wide_keys,
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
) -> (
    indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>>,
    indexmap::IndexMap<String, usize>,
) {
    let mut sig_per_crate: indexmap::IndexMap<String, indexmap::IndexMap<Pattern, usize>> =
        indexmap::IndexMap::new();
    let mut top_n_per_crate: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
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
        let sig = bucket_and_cap_by_group(&filtered, set, base_cap, cap_matrix, floor);
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
/// sets).
fn bucket_and_cap_by_group<V>(
    counts: &indexmap::IndexMap<Pattern, V>,
    set: PickSet,
    base_cap: usize,
    matrix: &CapMatrix,
    floor: usize,
) -> indexmap::IndexMap<Pattern, V>
where
    V: Clone + PartialOrd,
{
    let mut by_group: HashMap<PickGroup, Vec<(Pattern, V)>> = HashMap::new();
    for (k, v) in counts {
        by_group
            .entry(k.kind())
            .or_default()
            .push((k.clone(), v.clone()));
    }
    let mut out: indexmap::IndexMap<Pattern, V> = indexmap::IndexMap::new();
    for (group, mut items) in by_group {
        let cap = matrix.cap_for(group, set, base_cap, floor);
        items.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        for (k, v) in items.into_iter().take(cap) {
            out.insert(k, v);
        }
    }
    out
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
