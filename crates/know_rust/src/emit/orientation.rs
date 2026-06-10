use crate::*;

/// What: render the orientation.md content for non-container workspaces
/// as a single string. Composes provenance + how-shaped + S1 crate /
/// region map + S2 core vocabulary + S3 seam spine + S4 dataflow
/// narrative + S5 six significance sub-sections + tier guidance +
/// authoring guide + S6 unresolved guardrails + S7 authoring guide +
/// histogram appendix.
///
/// Why: emit.py's `emit_orientation()` (lines 1246-1700). The
/// read-first map artifact the agent loads to author against the
/// workspace. Mechanical skeleton + clearly marked [AGENT] slots the
/// agent fills by reading source at the cited spans.
///
/// Where: dispatched by `crate::emit::run::emit` when
/// `workspace_shape.shape != container`; the container-shaped path
/// uses `crate::emit::container_routing::render_container_routing`
/// instead.
pub fn render_orientation(
    workspace_root: &Path,
    out_dir: &Path,
    fp: &serde_json::Value,
    facts: &serde_json::Value,
    calibration: &Calibration,
    templates: &Templates,
    weights: Option<&TargetWeights>,
    profile_name: &str,
    profile: &ProfileSetScale,
) -> String {
    let sel = fp.get("selection").cloned().unwrap_or(serde_json::Value::Null);
    let vocab = core_vocabulary(fp, facts);
    let cands = candidate_instances(fp, facts, calibration, weights, profile);
    let forecast_chars = cands.total_budget_chars();
    let forecast_tokens = forecast_chars / 4;
    eprintln!(
        "[emit] kp forecast (profile {}): {}K tokens ({}K chars; sum of budget_hints across surviving picks per R4b cap matrix)",
        profile_name,
        forecast_tokens / 1000,
        forecast_chars / 1000,
    );
    let seams = detected_seams(fp, facts);

    let mut lines: Vec<String> = Vec::new();
    let header_ctx = OrientationHeaderContext {
        provenance: provenance(workspace_root, out_dir, fp),
    };
    let header_text = templates
        .render_prompt("orientation_header", &header_ctx)
        .expect("bundled orientation_header.liquid is well-formed");
    lines.push(header_text.trim_end_matches('\n').to_string());
    lines.push(String::new());

    // How shaped
    lines.push("## How this artifact was shaped".to_string());
    lines.push(String::new());
    let mode = sel.get("mode").and_then(|v| v.as_str()).unwrap_or("?");
    let histogram_mode = sel.get("histogram_mode").and_then(|v| v.as_str()).unwrap_or("?");
    let top_share = sel
        .get("top_share")
        .map(format_value_python)
        .unwrap_or_else(|| "?".to_string());
    lines.push(format!(
        "- mode: **{}** (histogram alone: {}, top_share={})",
        mode, histogram_mode, top_share
    ));
    let shape_info = fp
        .get("workspace_shape")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let shape_label = shape_info.get("shape").and_then(|v| v.as_str()).map(String::from);
    if let Some(label) = &shape_label {
        if label != "container" {
            let reasoning = shape_info.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!(
                "- structural shape: **{}** -- {}",
                label, reasoning
            ));
        }
    }
    let use_info = fp
        .get("workspace_use_classification")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let use_label = use_info
        .get("workspace")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(label) = &use_label {
        let reasoning = use_info.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");
        lines.push(format!(
            "- use classification: **{}** -- {}",
            label, reasoning
        ));
    }
    if let Some(ru) = sel.get("runner_up").and_then(|v| v.as_str()) {
        lines.push(format!(
            "- **UNRESOLVED (method-selection):** runner-up mode `{}` is within the ambiguity \
             band. Confirm against the histogram below.",
            ru
        ));
    }
    if let Some(notes) = sel.get("notes").and_then(|v| v.as_array()) {
        for n in notes {
            if let Some(s) = n.as_str() {
                lines.push(format!("- note: {}", s));
            }
        }
    }
    lines.push(String::new());

    // 1. Crate / region map
    lines.push("## 1. Crate / region map".to_string());
    lines.push(String::new());
    let per_crate = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let cluster_threshold = calibration.picker.cluster.threshold;
    let cluster_min_size = calibration.picker.cluster.min_size;
    let use_clusters_pref = per_crate.len() > cluster_threshold;
    let crate_names: Vec<String> = per_crate.keys().cloned().collect();
    let (clusters, others) = if use_clusters_pref {
        cluster_crates_by_prefix(&crate_names, cluster_min_size)
    } else {
        (indexmap::IndexMap::new(), Vec::new())
    };
    let use_clusters = use_clusters_pref && !clusters.is_empty();

    let n_components = fp.get("n_components").and_then(|v| v.as_i64()).unwrap_or(0);
    let workspace_roots_len = fp
        .get("workspace_roots")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if n_components > 1 || workspace_roots_len > 1 {
        lines.push(format!(
            "**Regional** - {} disjoint component(s), {} workspace root(s). Each component is a \
             region; the seam-spine (S3) is the join. **[AGENT]** name each region's role and \
             the named seams connecting it to the others; if two regions share no traced data \
             path, record that as an UNRESOLVED rather than inventing a link.",
            n_components, workspace_roots_len
        ));
        lines.push(String::new());
        let components = fp
            .get("components")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for (idx, comp) in components.iter().enumerate() {
            let comp_names: Vec<String> = comp
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            lines.push(format!("- region {}: {}", idx + 1, comp_names.join(", ")));
        }
    } else {
        lines.push("Single connected component. Crates and their internal dependencies:".to_string());
    }
    lines.push(String::new());

    // Workspace units: identity = provenance. Rendered whenever the
    // repo carries more than the host unit so the consuming agent
    // sees, structurally, that a vendored unit is a DISTINCT IDENTITY
    // from any same-named project (pop-os/iced is not iced-rs/iced).
    // Absent on pre-identity fingerprints.
    let units = fp
        .get("workspace_units")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if units.len() > 1 {
        lines.push(
            "Workspace units (identity = provenance; a vendored unit is a distinct identity \
             from any same-named project):"
                .to_string(),
        );
        let mut keys: Vec<String> = units.keys().cloned().collect();
        keys.sort();
        if let Some(pos) = keys.iter().position(|k| k == ".") {
            let host = keys.remove(pos);
            keys.insert(0, host);
        }
        for k in &keys {
            let u = &units[k];
            let kind = u
                .pointer("/provenance/kind")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let members = u
                .get("members")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let populated = u
                .get("populated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let line = match kind {
                "host" => format!("- `{}` - host workspace ({} members)", k, members),
                "submodule" => {
                    let url = u
                        .pointer("/provenance/url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let rev = u
                        .pointer("/provenance/rev")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    if populated {
                        format!(
                            "- `{}` - vendored submodule {} @ {} ({} members scanned)",
                            k, url, rev, members
                        )
                    } else {
                        format!(
                            "- `{}` - vendored submodule {} @ {} (source not populated in this \
                             clone; not scanned)",
                            k, url, rev
                        )
                    }
                }
                _ => {
                    if populated {
                        format!(
                            "- `{}` - in-repo vendored source ({} members scanned)",
                            k, members
                        )
                    } else {
                        format!(
                            "- `{}` - in-repo vendored source (not populated; not scanned)",
                            k
                        )
                    }
                }
            };
            lines.push(line);
        }
        lines.push(String::new());
    }

    if use_clusters {
        lines.push("### 1.1 Crate clusters (by name prefix)".to_string());
        lines.push(String::new());
        lines.push(format!(
            "Workspace has {} crates; the prefix-grouping below surfaces architectural clusters \
             above the per-crate detail. Threshold for clustering: {} crates (env: \
             ORIENT_CLUSTER_THRESHOLD). Minimum cluster size: {} (env: ORIENT_CLUSTER_MIN_SIZE). \
             Snake-case (`name_x`) and kebab-case (`name-x`) prefixes are kept distinct.",
            per_crate.len(), cluster_threshold, cluster_min_size
        ));
        lines.push(String::new());
        let mut cluster_prefixes: Vec<String> = clusters.keys().cloned().collect();
        cluster_prefixes.sort();
        for prefix in &cluster_prefixes {
            let mut members = clusters.get(prefix).cloned().unwrap_or_default();
            members.sort();
            let sep = if prefix.contains('_') { '_' } else { '-' };
            lines.push(format!(
                "- **`{}{}*`** ({} crates): {}",
                prefix, sep, members.len(), members.join(", ")
            ));
        }
        if !others.is_empty() {
            lines.push(format!(
                "- **Other** ({} crates): {}",
                others.len(), others.join(", ")
            ));
        }
        lines.push(String::new());
        lines.push("### 1.2 Per-crate detail".to_string());
        lines.push(String::new());
    }
    let mut crate_names_sorted: Vec<String> = per_crate.keys().cloned().collect();
    crate_names_sorted.sort();
    for name in &crate_names_sorted {
        let c = match per_crate.get(name) {
            Some(c) => c,
            None => continue,
        };
        let dir = c.get("dir").and_then(|v| v.as_str()).unwrap_or("?");
        let sloc = c.get("sloc").and_then(|v| v.as_i64()).unwrap_or(0);
        let n_impls = c.get("n_impls").and_then(|v| v.as_i64()).unwrap_or(0);
        let n_types = c.get("n_types").and_then(|v| v.as_i64()).unwrap_or(0);
        let ideps: Vec<String> = c
            .get("deps")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|d| d.as_str().map(String::from))
                    .filter(|d| per_crate.contains_key(d))
                    .collect()
            })
            .unwrap_or_default();
        let dep_str = if ideps.is_empty() {
            String::new()
        } else {
            format!(" -> depends on: {}", ideps.join(", "))
        };
        lines.push(format!(
            "- **{}** ({}/, {} SLOC, {} impls, {} types){}",
            name, dir, sloc, n_impls, n_types, dep_str
        ));
    }
    lines.push(String::new());
    if use_clusters {
        lines.push(
            "**[AGENT]** In 2-4 sentences each, describe each crate cluster (S1.1) and the \
             load-bearing standalone crates from S1.2 / 'Other'. Populate *why* only from \
             crate-level doc-comments / README; where absent, write `why: unverified`.".to_string(),
        );
    } else {
        lines.push(
            "**[AGENT]** In 2-4 sentences each (what / why / where), describe the role of the \
             core crates. Populate *why* only from crate-level doc-comments / README; where \
             absent, write `why: unverified`.".to_string(),
        );
    }
    lines.push(String::new());

    // 2. Core type vocabulary
    lines.push("## 2. Core type vocabulary".to_string());
    lines.push(String::new());
    let core_str = vocab.core.clone().unwrap_or_else(|| "None".to_string());
    lines.push(format!(
        "Most-depended-on crate: **{}** - its public types are the vocabulary other crates \
         speak in. Items below are ranked by impl-block usage (descending) with \
         alphabetical-by-name as the tiebreaker (0.0.4 patch 2; was alphabetical at 0.0.3). \
         Confirm and describe each (what / where load-bearing; why from doc-comments else \
         unverified):",
        core_str
    ));
    lines.push(String::new());
    for (t, usage) in vocab.traits.iter().take(40) {
        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let usage_str = if *usage > 0 {
            format!(
                "  *({} impl{})*",
                usage, if *usage != 1 { "s" } else { "" }
            )
        } else {
            String::new()
        };
        let doc = match t.get("doc").and_then(|v| v.as_str()) {
            Some(d) if !d.is_empty() => format!(" - doc: {}", truncate_chars(d, 120)),
            _ => "  *(why: unverified - no doc)*".to_string(),
        };
        lines.push(format!(
            "- trait `{}` - {}{}{}",
            name, span(t), usage_str, doc
        ));
    }
    for (t, usage) in vocab.types.iter().take(40) {
        let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let usage_str = if *usage > 0 {
            format!(
                "  *({} usage{})*",
                usage, if *usage != 1 { "s" } else { "" }
            )
        } else {
            String::new()
        };
        let doc = match t.get("doc").and_then(|v| v.as_str()) {
            Some(d) if !d.is_empty() => format!(" - doc: {}", truncate_chars(d, 120)),
            _ => "  *(why: unverified - no doc)*".to_string(),
        };
        lines.push(format!(
            "- `{} {}` - {}{}{}",
            kind, name, span(t), usage_str, doc
        ));
    }
    lines.push(String::new());

    // 3. Seam-spine
    lines.push("## 3. Seam-spine".to_string());
    lines.push(String::new());
    lines.push(
        "Where the workspace stops being one connected thing. These are where architect-".to_string(),
    );
    lines.push(
        "level features add or cross boundaries, and where the static trace stops honestly.".to_string(),
    );
    lines.push(String::new());
    if !seams.is_empty() {
        for s in &seams {
            lines.push(format!("- **{}** - {}", s.title, s.description));
            for site in &s.sites {
                let path = site.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let file = site.get("file").and_then(|v| v.as_str()).unwrap_or("?");
                let line = site.get("line").and_then(|v| v.as_i64());
                let line_str = match line {
                    Some(l) => l.to_string(),
                    None => "?".to_string(),
                };
                lines.push(format!("    - site: {} ({}:{})", path, file, line_str));
            }
        }
    } else {
        lines.push(
            "- No strong seam markers detected. **[AGENT]** confirm by inspecting the dominant \
             pattern's boundaries; absence of markers is itself worth noting.".to_string(),
        );
    }
    lines.push(String::new());
    lines.push(
        "**[AGENT]** For each seam, trace each protagonist pattern's instance (per S5.N) UP TO \
         the seam and stop. Record the wire / foreign contract location if visible; otherwise \
         write `UNRESOLVED: <what you looked for>, <what you ran>`. Different protagonists may \
         interact with the same seam differently (e.g. plugin commands cross the IPC seam; \
         builtin commands do not).".to_string(),
    );
    lines.push(String::new());

    // 4. Data-flow narrative (agent)
    let s4_text = templates
        .render_prompt("orientation_s4_dataflow", &EmptyContext {})
        .expect("bundled orientation_s4_dataflow.liquid is well-formed");
    lines.push(s4_text.trim_end_matches('\n').to_string());
    lines.push(String::new());

    // 5. Significance sets
    let top_n_workspace = cands.top_n_workspace;
    let all_per_crate_top_ns: Vec<usize> = cands
        .top_n_intra_crate_per_crate
        .values()
        .copied()
        .chain(cands.top_n_inner_per_crate.values().copied())
        .collect();
    let per_crate_min = all_per_crate_top_ns
        .iter()
        .copied()
        .min()
        .unwrap_or(calibration.picker.top_n_floor);
    let per_crate_max = all_per_crate_top_ns
        .iter()
        .copied()
        .max()
        .unwrap_or(calibration.picker.top_n_floor);

    lines.push("## 5. Significance sets - the authoring templates".to_string());
    lines.push(String::new());
    let per_crate_summary = if per_crate_min == per_crate_max {
        format!("per-crate base cap {}", per_crate_min)
    } else {
        format!("per-crate base cap range {}..{}", per_crate_min, per_crate_max)
    };
    lines.push(format!(
        "R4b cap matrix applied per (PickGroup, PickSet). Workspace-wide base cap {}; {}. \
         SLOC-scaled base via `max({}, round({} + {} * log2(SLOC / {})))` (the_user \
         2026-06-04: 'i'd rather slightly over-produce than under produce'); per-group cell \
         weights in `[picker.cap_matrix.*]`. Each section's '(N significant)' count reflects \
         the post-matrix cap.",
        top_n_workspace,
        per_crate_summary,
        calibration.picker.top_n_floor,
        calibration.picker.top_n_floor,
        calibration.picker.sloc_multiplier,
        calibration.picker.sloc_divisor,
    ));
    lines.push(String::new());

    let has_any = !cands.architecture.is_empty()
        || !cands.public.is_empty()
        || !cands.inter_crate.is_empty()
        || !cands.clique.is_empty()
        || !cands.intra_crate_per_crate.is_empty()
        || !cands.inner_crate_per_crate.is_empty();
    if has_any {
        // 5.1 Architecture
        lines.push(format!(
            "### 5.1 Architecture significance ({} significant; workspace-wide cross-crate AND \
             public-by-example)",
            cands.architecture.len()
        ));
        lines.push(String::new());
        for entry in sorted_entries_desc(&cands.architecture) {
            lines.push(format!(
                "- `{}` - architecture score {} - seed {}{}",
                entry.pattern,
                format_float_2(entry.count),
                span(&entry.instance),
                budget_suffix(&entry),
            ));
        }
        lines.push(String::new());
        // 5.2 Public
        lines.push(format!(
            "### 5.2 Public significance ({} significant; workspace-wide public-by-example only)",
            cands.public.len()
        ));
        lines.push(String::new());
        for entry in sorted_entries_desc(&cands.public) {
            lines.push(format!(
                "- `{}` - public score {} - seed {}{}",
                entry.pattern,
                format_float_2(entry.count),
                span(&entry.instance),
                budget_suffix(&entry),
            ));
        }
        lines.push(String::new());
        // 5.3 Inter-crate
        lines.push(format!(
            "### 5.3 Inter-crate significance ({} significant; workspace-wide cross-crate flow \
             only)",
            cands.inter_crate.len()
        ));
        lines.push(String::new());
        for entry in sorted_entries_desc(&cands.inter_crate) {
            lines.push(format!(
                "- `{}` - inter_count {} - seed {}{}",
                entry.pattern,
                format_count_int(entry.count),
                span(&entry.instance),
                budget_suffix(&entry),
            ));
        }
        lines.push(String::new());
        // 5.4 Clique. Sets a profile scales to zero are OFF: their
        // sections are omitted entirely (a consumer-profile bundle
        // carries no per-crate internals), not rendered empty.
        if profile.for_set(PickSet::Clique) > 0.0 {
            lines.push(format!(
                "### 5.4 Clique significance ({} elected; workspace-wide STV over per-crate intra \
                 ballots)",
                cands.clique.len()
            ));
            lines.push(String::new());
            for entry in sorted_entries_desc(&cands.clique) {
                lines.push(format!(
                    "- `{}` - clique votes {} - seed {}{}",
                    entry.pattern,
                    format_float_2(entry.count),
                    span(&entry.instance),
                    budget_suffix(&entry),
                ));
            }
            lines.push(String::new());
        }
        // 5.5 Intra-crate
        if profile.for_set(PickSet::IntraCrate) > 0.0 {
            lines.push(
                "### 5.5 Intra-crate significance (per crate; patterns this crate uses with origin \
                 in OTHER workspace crates, after dedup vs clique)".to_string(),
            );
            lines.push(String::new());
            let mut intra_crates: Vec<String> =
                cands.intra_crate_per_crate.keys().cloned().collect();
            intra_crates.sort();
            for crate_name in &intra_crates {
                let entries = match cands.intra_crate_per_crate.get(crate_name) {
                    Some(e) if !e.is_empty() => e,
                    _ => continue,
                };
                lines.push(format!(
                    "#### crate `{}` ({} significant)",
                    crate_name, entries.len()
                ));
                lines.push(String::new());
                for entry in sorted_entries_desc(entries) {
                    lines.push(format!(
                        "- `{}` - {} occurrences - seed {}{}",
                        entry.pattern,
                        format_count_int(entry.count),
                        span(&entry.instance),
                        budget_suffix(&entry),
                    ));
                    lines.push(String::new());
                }
            }
        }
        // 5.6 Inner-crate
        if profile.for_set(PickSet::InnerCrate) > 0.0 {
            lines.push(
                "### 5.6 Inner-crate significance (per crate; patterns originating IN and used IN \
                 this crate - the crate's own architecture)".to_string(),
            );
            lines.push(String::new());
            let mut inner_crates: Vec<String> =
                cands.inner_crate_per_crate.keys().cloned().collect();
            inner_crates.sort();
            for crate_name in &inner_crates {
                let entries = match cands.inner_crate_per_crate.get(crate_name) {
                    Some(e) if !e.is_empty() => e,
                    _ => continue,
                };
                lines.push(format!(
                    "#### crate `{}` ({} significant)",
                    crate_name, entries.len()
                ));
                lines.push(String::new());
                for entry in sorted_entries_desc(entries) {
                    lines.push(format!(
                        "- `{}` - {} occurrences - seed {}{}",
                        entry.pattern,
                        format_count_int(entry.count),
                        span(&entry.instance),
                        budget_suffix(&entry),
                    ));
                    lines.push(String::new());
                }
            }
        }
        // Tier guidance
        let tier_text = templates
            .render_prompt("orientation_s5_tier_guidance", &EmptyContext {})
            .expect("bundled orientation_s5_tier_guidance.liquid is well-formed");
        lines.push(tier_text.trim_end_matches('\n').to_string());
        lines.push(String::new());
        if let Some(ul) = &use_label {
            let modifier_name = match ul.as_str() {
                "dev_use" => Some("orientation_s5_use_tier_dev_use"),
                "end_with_dev_use" => Some("orientation_s5_use_tier_end_with_dev_use"),
                "dev_with_end_use" => Some("orientation_s5_use_tier_dev_with_end_use"),
                "end_use" => Some("orientation_s5_use_tier_end_use"),
                _ => None,
            };
            if let Some(name) = modifier_name {
                let mod_text = templates
                    .render_prompt(name, &EmptyContext {})
                    .expect("bundled use_tier_modifier liquid is well-formed");
                lines.push(mod_text.trim_end_matches('\n').to_string());
                lines.push(String::new());
            }
        }
        let auth_text = templates
            .render_prompt("orientation_s5_authoring_guidance", &EmptyContext {})
            .expect("bundled orientation_s5_authoring_guidance.liquid is well-formed");
        lines.push(auth_text.trim_end_matches('\n').to_string());
        lines.push(String::new());
    }
    lines.push(String::new());

    // 5F. Foreign API surface: the re-exported non-workspace items
    // a consumer legitimately reaches THROUGH this workspace. The
    // demand trace classifies these into the non-gating
    // foreign_reexport bucket; this section is the serving half -
    // the reader sees what the surface re-exports without chasing
    // the foreign source.
    let foreign = foreign_api_surface(fp, facts);
    if !foreign.is_empty() {
        lines.push("## 5F. Foreign API surface (re-exported)".to_string());
        lines.push(String::new());
        lines.push(
            "Items below are NOT workspace-defined: the workspace re-exports them from \
             foreign crates (std included) as part of its public face. Authoring against \
             them follows the FOREIGN crate's contract; the workspace controls only the \
             re-export path. Grouped by foreign root; `(namespace)` marks whole-crate \
             re-exports whose full surface lives in the foreign crate's own docs."
                .to_string(),
        );
        lines.push(String::new());
        for (root, entry) in &foreign {
            let ns = if entry.namespace { " (namespace)" } else { "" };
            if entry.leaves.is_empty() {
                lines.push(format!("- **`{}`**{}", root, ns));
            } else {
                let shown: Vec<String> =
                    entry.leaves.iter().take(20).cloned().collect();
                let tail = if entry.leaves.len() > 20 {
                    format!(" (+{} more)", entry.leaves.len() - 20)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "- **`{}`**{}: {}{}",
                    root,
                    ns,
                    shown.join(", "),
                    tail
                ));
            }
        }
        lines.push(String::new());
    }

    // 6. UNRESOLVED guardrails
    lines.push("## 6. UNRESOLVED guardrails".to_string());
    lines.push(String::new());
    lines.push(
        "Do not author *across* these without verifying in source first - a guessed bridge \
         compiles but is wrong. Seeded from detected boundaries; **[AGENT]** add any trace stop \
         you hit.".to_string(),
    );
    lines.push(String::new());
    let registration_macros = fp
        .get("registration_macros")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    if !registration_macros.is_empty() {
        let defs_idx = macro_defs_index(facts);
        for (mac, _n) in registration_macros.iter() {
            let defs = defs_idx.get(mac).cloned().unwrap_or_default();
            if !defs.is_empty() {
                let shown: Vec<(String, i64)> = defs.iter().take(5).cloned().collect();
                let tail = if defs.len() <= 5 {
                    String::new()
                } else {
                    format!(" (+ {} more definition site(s))", defs.len() - 5)
                };
                let spans_str: Vec<String> = shown
                    .iter()
                    .map(|(f, l)| format!("{}:{}", f, l))
                    .collect();
                let spans_joined = spans_str.join(", ");
                lines.push(format!(
                    "- **`{}!` registration** - expansion READABLE inline in the workspace at \
                     {}{}. Call-site arg counts remain unverified without the rustdoc overlay, \
                     but the `macro_rules!` body is statically followable for each listed \
                     definition site. Different definition crates may expand differently; \
                     confirm per call-site crate.",
                    mac, spans_joined, tail
                ));
            } else {
                lines.push(format!(
                    "- **`{}!` registration** - expansion not visible to the floor scanner (no \
                     inline `macro_rules!` definition found in the workspace; may be a \
                     proc-macro, imported from an external crate, or otherwise out of scope). \
                     Call-site arg counts are unverified. Confirm generated items in source or \
                     via the rustdoc overlay before relying on the registry.",
                    mac
                ));
            }
        }
    }
    for s in &seams {
        lines.push(format!("- **{}** - {}", s.title, s.description));
    }
    if registration_macros.is_empty() && seams.is_empty() {
        lines.push("- None seeded. Record trace stops here as you hit them.".to_string());
    }
    lines.push(String::new());

    // 7. Pattern-authoring guides
    let cls_tag = match &use_label {
        Some(l) => format!("Workspace classification: **{}**. ", l),
        None => String::new(),
    };
    let s7_ctx = OrientationS7Context {
        classification_tag: cls_tag,
    };
    let s7_text = templates
        .render_prompt("orientation_s7_authoring", &s7_ctx)
        .expect("bundled orientation_s7_authoring.liquid is well-formed");
    lines.push(s7_text.trim_end_matches('\n').to_string());
    lines.push(String::new());

    // Appendix
    lines.push("## Appendix: full pattern histogram".to_string());
    lines.push(String::new());
    let histogram = fp
        .get("pattern_histogram")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for row in histogram.iter().take(25) {
        let pattern = row.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let count = row.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        lines.push(format!("- `{}` - {}", pattern, count));
    }
    lines.push(String::new());

    lines.join("\n")
}

/// What: one foreign root's re-exported surface - whether the whole
/// namespace is re-exported and the sorted distinct leaf names.
pub(crate) struct ForeignRootSurface {
    pub(crate) namespace: bool,
    pub(crate) leaves: Vec<String>,
}

/// What: build the foreign-API surface from the facts' `pub use`
/// entries: every re-export whose ROOT is neither a workspace crate
/// (package or lib-rename binding, per the fingerprint's per_crate)
/// nor a crate/self/super self-reference groups under its foreign
/// root - std/core/alloc count as foreign. Returns root ->
/// (namespace flag, sorted leaves), sorted by root.
///
/// Why: the demand side's foreign_reexport bucket reports this class
/// as structurally unservable by workspace picks; this builder feeds
/// the orientation section that SERVES it instead, using the same
/// detection semantics so the two halves cannot drift.
///
/// Where: called by `render_orientation` for the 5F section.
pub(crate) fn foreign_api_surface(
    fp: &serde_json::Value,
    facts: &serde_json::Value,
) -> std::collections::BTreeMap<String, ForeignRootSurface> {
    let mut ws_roots: HashSet<String> = HashSet::new();
    if let Some(pc) = fp.get("per_crate").and_then(|v| v.as_object()) {
        for (name, c) in pc {
            ws_roots.insert(name.replace('-', "_"));
            if let Some(ln) = c.get("lib_name").and_then(|v| v.as_str()) {
                ws_roots.insert(ln.replace('-', "_"));
            }
        }
    }
    // Local-module gate (mirrors the demand side): a relative
    // re-export path roots at the crate's own module, not a foreign
    // crate.
    let mut mods_by_crate: std::collections::HashMap<String, HashSet<String>> =
        std::collections::HashMap::new();
    let empty: Vec<serde_json::Value> = Vec::new();
    for m in facts.get("mods").and_then(|v| v.as_array()).unwrap_or(&empty) {
        if let (Some(n), Some(c)) = (
            m.get("name").and_then(|v| v.as_str()),
            m.get("crate").and_then(|v| v.as_str()),
        ) {
            mods_by_crate
                .entry(c.to_string())
                .or_default()
                .insert(n.to_string());
        }
    }
    let mut out: std::collections::BTreeMap<String, ForeignRootSurface> =
        std::collections::BTreeMap::new();
    for u in facts.get("uses").and_then(|v| v.as_array()).unwrap_or(&empty) {
        if !u.get("reexport").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(path) = u.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        if !is_src_file(u.get("file").and_then(|v| v.as_str()).unwrap_or("")) {
            continue;
        }
        let parsed = parse_use_leaves(path);
        let root_norm = parsed.root.replace('-', "_");
        if parsed.root.is_empty()
            || ["crate", "self", "super"].contains(&parsed.root.as_str())
            || ws_roots.contains(&root_norm)
        {
            continue;
        }
        let local_mod = u
            .get("crate")
            .and_then(|v| v.as_str())
            .and_then(|c| mods_by_crate.get(c))
            .map(|s| s.contains(&parsed.root))
            .unwrap_or(false);
        if local_mod {
            continue;
        }
        let entry = out.entry(parsed.root.clone()).or_insert(ForeignRootSurface {
            namespace: false,
            leaves: Vec::new(),
        });
        if !path.trim().contains("::") {
            entry.namespace = true;
            continue;
        }
        for leaf in &parsed.leaves {
            if let UseLeaf::Named { binding, .. } = leaf {
                if !entry.leaves.contains(binding) {
                    entry.leaves.push(binding.clone());
                }
            }
        }
    }
    for e in out.values_mut() {
        e.leaves.sort();
    }
    out
}

/// What: sort an IndexMap of pattern -> EnrichedEntry by descending
/// count, stable in pre-existing insertion order on count ties.
/// Returns the entries vector ready for sequential rendering.
///
/// Why: emit.py's `sorted(set.keys(), key=lambda p: -set[p]["count"])`
/// idiom. Python's sorted is stable; ties preserve insertion order
/// (which is the picker's already-descending output order). Rust's
/// stable sort gives the same behavior on ties.
///
/// Where: used inside the S5 sub-section loops to emit bullets in
/// canonical order.
fn sorted_entries_desc(
    entries: &indexmap::IndexMap<Pattern, EnrichedEntry>,
) -> Vec<EnrichedEntry> {
    let mut items: Vec<EnrichedEntry> = entries.values().cloned().collect();
    items.sort_by(|a, b| {
        b.count
            .partial_cmp(&a.count)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    items
}

/// What: format an f64 like Python's `f"{x:.2f}"` - two decimal places
/// rounded half-to-even via the format machinery's default.
///
/// Why: emit.py renders architecture / public / clique counts with
/// `:.2f`; matching format keeps canonicalized output byte-equal.
fn format_float_2(x: f64) -> String {
    format!("{:.2}", x)
}

/// What: format an EnrichedEntry's count as an integer when the source
/// set was usize-based (inter / intra / inner). Python f-string of an
/// int looks like `{:d}`; here the f64 is rounded down to i64 first.
fn format_count_int(x: f64) -> String {
    format!("{}", x as i64)
}

/// What: format a JSON value the way Python's f"{val}" would for the
/// `top_share` shown in the how-shaped section.
fn format_value_python(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(f) = n.as_f64() {
                if f.is_nan() {
                    "nan".to_string()
                } else if f.is_infinite() {
                    if f > 0.0 { "inf".to_string() } else { "-inf".to_string() }
                } else if f == f.trunc() && f.abs() < 1e16 {
                    format!("{}.0", f as i64)
                } else {
                    format!("{}", f)
                }
            } else {
                "?".to_string()
            }
        }
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

/// What: truncate a string at `n` characters (Python's `s[:n]`
/// semantics for str). Counts unicode scalar values, not bytes.
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// What: render the R4 prose-budget suffix for an S5 pick bullet.
/// Format: ` - budget N (sub_form)` when sub_form is classified,
/// ` - budget N` when sub_form is None, empty string when budget is
/// zero (no matrix row applies, e.g. unparseable pattern).
///
/// Why: the kp pipeline Stage C drafting subagent reads orientation.md
/// per pick and needs the budget hint visible inline; the sub_form
/// annotation helps debugging when classifier output diverges from
/// expectations on a target.
///
/// Where: appended to each S5.1 / 5.2 / 5.3 / 5.4 / 5.5 / 5.6 bullet
/// inside `render_orientation`.
fn budget_suffix(entry: &EnrichedEntry) -> String {
    if entry.budget_hint == 0 {
        return String::new();
    }
    match entry.sub_form {
        Some(sf) => format!(" - budget {} ({})", entry.budget_hint, sf.wire()),
        None => format!(" - budget {}", entry.budget_hint),
    }
}
