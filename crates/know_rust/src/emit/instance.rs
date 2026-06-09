use crate::*;

/// What: one enriched picker entry - pattern + per-set score (count or
/// float) + the seed instance dict + R4 prose budget hint + classified
/// sub-form. The pattern is the full `group:name` string; the
/// orientation composer renders the pattern verbatim and the instance
/// is the structurally-followable seed.
///
/// Why: emit.py's `candidate_instances` enriches each picker set entry
/// with a seed instance the agent can open; the S5 sub-sections in
/// orientation.md render bullets shaped `group:name - <score> - seed
/// <span> - budget N [(sub_form)]` from this struct. R4 adds the
/// budget hint so the kp pipeline Stage C drafting subagent sees the
/// per-pick char target inline alongside the pattern + seed.
///
/// Where: produced by `crate::emit::instance::candidate_instances`;
/// consumed by `crate::emit::orientation::render_orientation`.
#[derive(Debug, Clone)]
pub struct EnrichedEntry {
    pub pattern: Pattern,
    pub count: f64,
    pub instance: serde_json::Value,
    pub budget_hint: usize,
    pub sub_form: Option<SubForm>,
}

/// What: full output of `candidate_instances` - the six picker sets
/// (architecture / public / inter_crate / clique workspace-wide +
/// intra_crate / inner_crate per-crate) enriched with seed instances,
/// plus the per-crate and workspace-level top-N caps.
///
/// Why: emit.py's `candidate_instances` return dict shape, ported to a
/// typed struct so the orientation composer can render each section
/// without re-deriving caps or re-enriching entries.
#[derive(Debug, Clone, Default)]
pub struct EnrichedSets {
    pub intra_crate_per_crate:
        indexmap::IndexMap<String, indexmap::IndexMap<Pattern, EnrichedEntry>>,
    pub inner_crate_per_crate:
        indexmap::IndexMap<String, indexmap::IndexMap<Pattern, EnrichedEntry>>,
    pub inter_crate: indexmap::IndexMap<Pattern, EnrichedEntry>,
    pub public: indexmap::IndexMap<Pattern, EnrichedEntry>,
    pub architecture: indexmap::IndexMap<Pattern, EnrichedEntry>,
    pub clique: indexmap::IndexMap<Pattern, EnrichedEntry>,
    pub top_n_intra_crate_per_crate: indexmap::IndexMap<String, usize>,
    pub top_n_inner_per_crate: indexmap::IndexMap<String, usize>,
    pub top_n_workspace: usize,
}

impl EnrichedSets {
    /// What: sum every surviving pick's `budget_hint` chars across
    /// the six sets. Pre-flight forecast for the assembled kp
    /// bundle's character count - divide by ~4 chars/token to get
    /// the token forecast.
    ///
    /// Why: R4b pre-flight forecast per
    /// `mem:know-rust-kp-output-token-target` - sum the prose_budget
    /// matrix outputs across surviving picks (post-R4b-cap-matrix)
    /// BEFORE running Stage A->D. Drives the matrix calibration
    /// sweep loop without needing to run the full subagent pipeline.
    ///
    /// Where: called from `crate::emit::orientation::render_orientation`
    /// after `candidate_instances` returns; the result is emitted as
    /// an `[emit] kp forecast` log line during every `know_rust emit`
    /// run.
    pub fn total_budget_chars(&self) -> usize {
        let mut total: usize = 0;
        for e in self.architecture.values() {
            total += e.budget_hint;
        }
        for e in self.public.values() {
            total += e.budget_hint;
        }
        for e in self.inter_crate.values() {
            total += e.budget_hint;
        }
        for e in self.clique.values() {
            total += e.budget_hint;
        }
        for set in self.intra_crate_per_crate.values() {
            for e in set.values() {
                total += e.budget_hint;
            }
        }
        for set in self.inner_crate_per_crate.values() {
            for e in set.values() {
                total += e.budget_hint;
            }
        }
        total
    }
}

/// What: pick a seed instance for a pattern of the given `(group, name)`
/// shape, plus up to 200 span strings of all matching facts. Returns
/// `(None, vec![])` for patterns with no actionable item under that
/// group.
///
/// Why: emit.py's `_instance_for_kind()` ported and migrated to the
/// picks-data model (R3 per
/// `notes/know_rust/tasks/picks-data-model-refactor.md`). Each picker
/// pattern (e.g. `traits:Component`, `configuring:Component`,
/// `utilities:cfg_attr_test_or_loom`,
/// `implementation_functions:_::update`,
/// `implementation_functions:World::new`, `structure:Frame`) needs a
/// seed instance the agent can open. The enriched picker output drops
/// patterns where this returns `(None, _)`.
///
/// Where: called from `candidate_instances` per pattern in each of the
/// six sets to produce the `EnrichedEntry` instance + all_spans
/// fields.
pub fn instance_for_kind(
    pattern: &Pattern,
    facts: &serde_json::Value,
) -> (Option<serde_json::Value>, Vec<String>) {
    let name_owned = pattern.name();
    let name = name_owned.as_str();
    let empty: Vec<serde_json::Value> = Vec::new();
    match pattern.kind() {
        PickGroup::Traits => {
            // Prefer impls of the trait (the impls-side architectural
            // signal); fall back to the trait def site when no impls
            // exist (e.g. pub_type:Plugin synthesized via AST refs
            // without any in-workspace impls).
            let impls_arr = facts.get("impls").and_then(|v| v.as_array()).unwrap_or(&empty);
            let mut inst: Vec<serde_json::Value> = impls_arr
                .iter()
                .filter(|i| {
                    i.get("trait").and_then(|v| v.as_str()) == Some(name)
                        && !i.get("cfg_gated").and_then(|v| v.as_bool()).unwrap_or(false)
                })
                .cloned()
                .collect();
            inst.sort_by(|a, b| {
                let ta = a
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "None".to_string());
                let tb = b
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| "None".to_string());
                ta.cmp(&tb)
            });
            if !inst.is_empty() {
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span).collect();
                return (first, spans);
            }
            let traits_arr = facts.get("traits").and_then(|v| v.as_array()).unwrap_or(&empty);
            let inst: Vec<serde_json::Value> = traits_arr
                .iter()
                .filter(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
                .cloned()
                .collect();
            let first = inst.first().cloned();
            let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
            (first, spans)
        }
        PickGroup::Configuring => {
            // Configured-via-attributes integration. Prefer derive
            // sites; fall back to attribute-macro invocation sites - the
            // broaden folds attr_macro into configuring, so a workspace-
            // defined attribute macro (e.g. a proc-macro the workspace
            // ships) seeds from the macros facts when no derive matches.
            let derives_arr =
                facts.get("derives").and_then(|v| v.as_array()).unwrap_or(&empty);
            let inst: Vec<serde_json::Value> = derives_arr
                .iter()
                .filter(|d| d.get("trait").and_then(|v| v.as_str()) == Some(name))
                .cloned()
                .collect();
            if !inst.is_empty() {
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
                return (first, spans);
            }
            let macros_arr =
                facts.get("macros").and_then(|v| v.as_array()).unwrap_or(&empty);
            let inst: Vec<serde_json::Value> = macros_arr
                .iter()
                .filter(|m| {
                    m.get("name").and_then(|v| v.as_str()) == Some(name)
                        && m.get("kind").and_then(|v| v.as_str()) == Some("attr_macro")
                })
                .cloned()
                .collect();
            let first = inst.first().cloned();
            let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
            (first, spans)
        }
        PickGroup::Utilities => {
            // Macros: either macro_invocation form or attr_macro form.
            // The picks-data model unifies both under utilities; the
            // seed prefers the more-common invocation form.
            let arr = facts.get("macros").and_then(|v| v.as_array()).unwrap_or(&empty);
            let inst: Vec<serde_json::Value> = arr
                .iter()
                .filter(|m| m.get("name").and_then(|v| v.as_str()) == Some(name))
                .cloned()
                .collect();
            let first = inst.first().cloned();
            let spans: Vec<String> = inst.iter().take(200).map(span).collect();
            (first, spans)
        }
        PickGroup::Structure => {
            // Prefer the type def (struct / enum / union / type alias);
            // fall back to a representative type_usage outer-prefix match
            // when the type def is not in workspace facts.
            let types_arr = facts.get("types").and_then(|v| v.as_array()).unwrap_or(&empty);
            let inst: Vec<serde_json::Value> = types_arr
                .iter()
                .filter(|t| t.get("name").and_then(|v| v.as_str()) == Some(name))
                .cloned()
                .collect();
            if !inst.is_empty() {
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
                return (first, spans);
            }
            let tu_arr = facts.get("type_usages").and_then(|v| v.as_array()).unwrap_or(&empty);
            let mut inst: Vec<serde_json::Value> = tu_arr
                .iter()
                .filter(|tu| {
                    let n = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    n.split_once("::").map(|(o, _)| o == name).unwrap_or(false)
                })
                .cloned()
                .collect();
            if inst.is_empty() {
                let ex = facts
                    .get("example_type_usages")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                inst = ex
                    .iter()
                    .filter(|tu| {
                        let n = tu.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        n.split_once("::").map(|(o, _)| o == name).unwrap_or(false)
                    })
                    .cloned()
                    .collect();
            }
            let first = inst.first().cloned();
            let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
            (first, spans)
        }
        PickGroup::ImplementationFunctions => {
            // Two name shapes:
            //   "Outer::inner" -> exact type_usage match.
            //   "_::<inner>"   -> method_ref family lookup (the picker
            //                     synthesizes this from ast_method_refs).
            if let Some(inner) = name.strip_prefix("_::") {
                let arr = facts
                    .get("ast_method_refs")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                let inst: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|r| r.get("inner").and_then(|v| v.as_str()) == Some(inner))
                    .cloned()
                    .collect();
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
                return (first, spans);
            }
            let arr = facts.get("type_usages").and_then(|v| v.as_array()).unwrap_or(&empty);
            let mut inst: Vec<serde_json::Value> = arr
                .iter()
                .filter(|tu| tu.get("name").and_then(|v| v.as_str()) == Some(name))
                .cloned()
                .collect();
            if inst.is_empty() {
                let ex = facts
                    .get("example_type_usages")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                inst = ex
                    .iter()
                    .filter(|tu| tu.get("name").and_then(|v| v.as_str()) == Some(name))
                    .cloned()
                    .collect();
            }
            let first = inst.first().cloned();
            let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
            (first, spans)
        }
        PickGroup::TraitFunctions => {
            // Mirrors implementation_functions for the family shape;
            // the disambiguation between trait-method-ref vs impl-method-ref
            // requires type inference (not available without rustdoc).
            // For now the family lookup pulls from ast_method_refs.
            if let Some(inner) = name.strip_prefix("_::") {
                let arr = facts
                    .get("ast_method_refs")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                let inst: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|r| r.get("inner").and_then(|v| v.as_str()) == Some(inner))
                    .cloned()
                    .collect();
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
                return (first, spans);
            }
            // "Trait::method" shape: no direct fact source today. The
            // walker's trait_functions carry covers the sig types but
            // not seed call sites; left empty until a future iteration.
            (None, Vec::new())
        }
        PickGroup::Globals => {
            // Associated-constant labels (R8 slice 3): the
            // `globals:<Outer>::<CONST>` picks synthesized from the
            // method-ref stream seed at their access sites.
            if let Some((outer, cname)) = name.split_once("::") {
                let arr = facts
                    .get("ast_method_refs")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                let inst: Vec<serde_json::Value> = arr
                    .iter()
                    .filter(|r| {
                        r.get("outer").and_then(|v| v.as_str()) == Some(outer)
                            && r.get("inner").and_then(|v| v.as_str()) == Some(cname)
                    })
                    .cloned()
                    .collect();
                let first = inst.first().cloned();
                let spans: Vec<String> = inst.iter().take(200).map(span_basic).collect();
                return (first, spans);
            }
            (None, Vec::new())
        }
    }
}

/// What: a `file:line` only span string (no end_line range), the
/// inline form used in `instance_for_kind` for several kinds where
/// Python explicitly inlines the format rather than calling `sp()`.
///
/// Why: emit.py's `_instance_for_kind` uses inline `f"{f}:{ln}"`
/// formatting for derive / type_usage / method_ref / pub_type /
/// type_usage_family span lists; this helper preserves the exact
/// per-kind formatting so canonicalized output stays byte-equal.
///
/// Where: internal helper for `instance_for_kind`.
fn span_basic(rec: &serde_json::Value) -> String {
    let file = rec.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = rec.get("line").and_then(|v| v.as_i64()).unwrap_or(-1);
    let line_str = if line < 0 {
        "?".to_string()
    } else {
        line.to_string()
    };
    format!("{}:{}", file, line_str)
}

/// What: the core vocabulary picker - most-depended-on in-workspace
/// crate plus its types and traits ranked by usage (descending) with
/// alphabetical-by-name tiebreaker. Filters to src/ files only.
///
/// Why: emit.py's `core_vocabulary()` (lines 394-458). Heuristic: the
/// core types live in the most-depended-on crate; the S2 vocabulary
/// should be the domain language other crates speak in, not a shared
/// error-helper or utility crate. Usage ranking surfaces load-bearing
/// types (Value, PipelineData, etc.) that alphabetical sort buries.
///
/// Where: called by `crate::emit::orientation::render_orientation` for
/// the S2 section header pick + the per-type / per-trait bullets.
pub fn core_vocabulary(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
) -> CoreVocabulary {
    let per_crate = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut dep_count: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    for c in per_crate.values() {
        if let Some(arr) = c.get("deps").and_then(|v| v.as_array()) {
            for d in arr {
                if let Some(s) = d.as_str() {
                    *dep_count.entry(s.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    let in_workspace: indexmap::IndexMap<String, usize> = dep_count
        .iter()
        .filter(|(k, _)| per_crate.contains_key(*k))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let pick_pool = if !in_workspace.is_empty() { in_workspace } else { dep_count };
    let core: Option<String> = if pick_pool.is_empty() {
        None
    } else {
        let mut best: Option<(String, usize)> = None;
        for (k, v) in &pick_pool {
            best = Some(match best {
                None => (k.clone(), *v),
                Some((_, bv)) if *v > bv => (k.clone(), *v),
                Some(prev) => prev,
            });
        }
        best.map(|(k, _)| k)
    };

    let empty: Vec<serde_json::Value> = Vec::new();
    let types_arr = facts.get("types").and_then(|v| v.as_array()).unwrap_or(&empty);
    let traits_arr = facts.get("traits").and_then(|v| v.as_array()).unwrap_or(&empty);
    let core_str = core.clone().unwrap_or_default();
    let types: Vec<serde_json::Value> = types_arr
        .iter()
        .filter(|t| {
            t.get("crate").and_then(|v| v.as_str()) == Some(core_str.as_str())
                && is_src_file(t.get("file").and_then(|v| v.as_str()).unwrap_or(""))
        })
        .cloned()
        .collect();
    let traits: Vec<serde_json::Value> = traits_arr
        .iter()
        .filter(|t| {
            t.get("crate").and_then(|v| v.as_str()) == Some(core_str.as_str())
                && is_src_file(t.get("file").and_then(|v| v.as_str()).unwrap_or(""))
        })
        .cloned()
        .collect();

    let mut trait_usage: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let mut type_usage: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    let impls_arr = facts.get("impls").and_then(|v| v.as_array()).unwrap_or(&empty);
    for i in impls_arr {
        if let Some(tr) = i.get("trait").and_then(|v| v.as_str()) {
            *trait_usage.entry(tr.to_string()).or_insert(0) += 1;
        }
        if let Some(ty) = i.get("type").and_then(|v| v.as_str()) {
            let bare = ty
                .split('<')
                .next()
                .unwrap_or("")
                .rsplit("::")
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !bare.is_empty() {
                *type_usage.entry(bare).or_insert(0) += 1;
            }
        }
    }
    let type_ident_re = regex::Regex::new(r"\b[A-Z]\w*\b").unwrap();
    let uses_arr = facts.get("uses").and_then(|v| v.as_array()).unwrap_or(&empty);
    for u in uses_arr {
        let path = u.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            continue;
        }
        for m in type_ident_re.find_iter(path) {
            *type_usage.entry(m.as_str().to_string()).or_insert(0) += 1;
        }
    }

    let mut traits_with_usage: Vec<(serde_json::Value, usize)> = traits
        .into_iter()
        .map(|t| {
            let n = t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let usage = trait_usage.get(&n).copied().unwrap_or(0);
            (t, usage)
        })
        .collect();
    traits_with_usage.sort_by(|a, b| {
        let ua = a.1 as i64;
        let ub = b.1 as i64;
        let na = a.0.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let nb = b.0.get("name").and_then(|v| v.as_str()).unwrap_or("");
        ub.cmp(&ua).then(na.cmp(nb))
    });
    let mut types_with_usage: Vec<(serde_json::Value, usize)> = types
        .into_iter()
        .map(|t| {
            let n = t.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let usage = type_usage.get(&n).copied().unwrap_or(0);
            (t, usage)
        })
        .collect();
    types_with_usage.sort_by(|a, b| {
        let ua = a.1 as i64;
        let ub = b.1 as i64;
        let na = a.0.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let nb = b.0.get("name").and_then(|v| v.as_str()).unwrap_or("");
        ub.cmp(&ua).then(na.cmp(nb))
    });

    CoreVocabulary {
        core,
        types: types_with_usage,
        traits: traits_with_usage,
    }
}

/// What: result of `core_vocabulary` - the picked most-depended-on
/// crate name (`None` if the workspace has no dependents at all),
/// plus its types and traits each paired with their usage count for
/// rendering alongside the entry.
#[derive(Debug, Clone, Default)]
pub struct CoreVocabulary {
    pub core: Option<String>,
    pub types: Vec<(serde_json::Value, usize)>,
    pub traits: Vec<(serde_json::Value, usize)>,
}

/// What: run the six-set significance picker and enrich each picked
/// pattern with an instance + span list via `instance_for_kind`.
/// Returns `EnrichedSets` consumed by the orientation composer.
///
/// Why: emit.py's `candidate_instances()` (lines 935-1029). Glues the
/// picker output to the section renderers; patterns whose instance
/// pick returns `None` are dropped so S5 sections show only
/// structurally-followable seeds.
///
/// Where: called by `crate::emit::orientation::render_orientation`
/// after `core_vocabulary` and `detected_seams` to produce the S5
/// material.
pub fn candidate_instances(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
    calibration: &Calibration,
) -> EnrichedSets {
    let histogram = fp
        .get("pattern_histogram")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if histogram.is_empty() {
        return EnrichedSets {
            top_n_workspace: calibration.picker.top_n_floor,
            ..EnrichedSets::default()
        };
    }
    let workspace_sloc = fp
        .get("totals")
        .and_then(|t| t.get("sloc"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let top_n_workspace = compute_sloc_scaled_top_n(workspace_sloc, calibration);
    let per_crate = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let mut per_crate_sloc: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    for (k, v) in per_crate.iter() {
        let s = v.get("sloc").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        per_crate_sloc.insert(k.clone(), s);
    }
    let sig = compute_significance_sets(fp, facts, &per_crate_sloc, top_n_workspace, calibration);

    let pattern_metrics = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let enrich_workspace = |pattern_map: &indexmap::IndexMap<Pattern, f64>, set: PickSet|
        -> indexmap::IndexMap<Pattern, EnrichedEntry>
    {
        let mut out: indexmap::IndexMap<Pattern, EnrichedEntry> = indexmap::IndexMap::new();
        for (pattern, count) in pattern_map.iter() {
            if let Some(entry) =
                build_enriched_entry(pattern, *count, set, facts, &pattern_metrics, calibration)
            {
                out.insert(pattern.clone(), entry);
            }
        }
        out
    };
    let enrich_workspace_int = |pattern_map: &indexmap::IndexMap<Pattern, usize>, set: PickSet|
        -> indexmap::IndexMap<Pattern, EnrichedEntry>
    {
        let mut out: indexmap::IndexMap<Pattern, EnrichedEntry> = indexmap::IndexMap::new();
        for (pattern, count) in pattern_map.iter() {
            if let Some(entry) = build_enriched_entry(
                pattern,
                *count as f64,
                set,
                facts,
                &pattern_metrics,
                calibration,
            ) {
                out.insert(pattern.clone(), entry);
            }
        }
        out
    };

    let mut intra_crate_per_crate: indexmap::IndexMap<
        String,
        indexmap::IndexMap<Pattern, EnrichedEntry>,
    > = indexmap::IndexMap::new();
    for (crate_name, s) in sig.significant_intra_crate_per_crate.iter() {
        intra_crate_per_crate
            .insert(crate_name.clone(), enrich_workspace_int(s, PickSet::IntraCrate));
    }
    let mut inner_crate_per_crate: indexmap::IndexMap<
        String,
        indexmap::IndexMap<Pattern, EnrichedEntry>,
    > = indexmap::IndexMap::new();
    for (crate_name, s) in sig.significant_inner_crate_per_crate.iter() {
        inner_crate_per_crate
            .insert(crate_name.clone(), enrich_workspace_int(s, PickSet::InnerCrate));
    }

    EnrichedSets {
        intra_crate_per_crate,
        inner_crate_per_crate,
        inter_crate: enrich_workspace_int(&sig.significant_inter_crate, PickSet::InterCrate),
        public: enrich_workspace(&sig.significant_public, PickSet::Public),
        architecture: enrich_workspace(&sig.significant_architecture, PickSet::Architecture),
        clique: enrich_workspace(&sig.significant_clique, PickSet::Clique),
        top_n_intra_crate_per_crate: sig.top_n_intra_crate_per_crate,
        top_n_inner_per_crate: sig.top_n_inner_per_crate,
        top_n_workspace,
    }
}

/// What: build one `EnrichedEntry` for a pick. Looks up the seed
/// instance from facts, reads the pre-classified `sub_form` from
/// `pattern_metrics`, and computes the prose budget hint via the
/// calibration matrix.
///
/// Why: factored out of `candidate_instances` so the enrichment is
/// reusable across all six sets without duplicating the
/// instance-lookup + matrix-lookup wiring per closure. Returns
/// `None` when no seed instance is available (the pick is dropped
/// from the enriched output).
///
/// Where: called by the `enrich_workspace` + `enrich_workspace_int`
/// closures inside `candidate_instances` for each pattern of each
/// significance set.
fn build_enriched_entry(
    pattern: &Pattern,
    count: f64,
    set: PickSet,
    facts: &serde_json::Value,
    pattern_metrics: &serde_json::Map<String, serde_json::Value>,
    calibration: &Calibration,
) -> Option<EnrichedEntry> {
    let (inst, _) = instance_for_kind(pattern, facts);
    let instance = inst?;
    let group = pattern.kind();
    let sub_form = pattern_metrics
        .get(&pattern.to_string())
        .and_then(|m| m.get("sub_form"))
        .and_then(|v| v.as_str())
        .and_then(SubForm::from_wire);
    let budget_hint = calibration.picker.prose_budget.budget_for(group, sub_form, set);
    Some(EnrichedEntry {
        pattern: pattern.clone(),
        count,
        instance,
        budget_hint,
        sub_form,
    })
}
