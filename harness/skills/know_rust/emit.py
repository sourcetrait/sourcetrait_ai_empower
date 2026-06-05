"""emit.py - final phase. Reads fingerprint.json + facts.json and writes the two-file
artifact:

  reference.md   exhaustive, span-anchored, grep-into. Fully mechanical.
  orientation.md read-first, map-first. The mechanical skeleton (provenance, crate/region
                 map, core type vocabulary, seam-spine, dominant-pattern identification,
                 a candidate instance, detected-seam UNRESOLVED guardrails) plus clearly
                 marked [AGENT ...] slots that Claude Code fills BY READING SOURCE at the
                 spans provided - the judgment work (data-flow narrative, the worked slice,
                 the why-axis from doc-comments) that no static tool can fabricate.

The division is deliberate: the tool produces everything that is fabrication-proof (counts,
spans, graph, detected boundaries); the agent authors everything requiring judgment, always
anchored to tool-provided spans so it cannot drift into fiction. Generation cost is ignored;
the reference is exhaustive.
"""

from __future__ import annotations
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import config  # noqa: E402

# 0.0.4 patch 2 (s): word-bounded capitalized identifier for use-path scanning.
_TYPE_IDENT_RE = re.compile(r'\b[A-Z]\w*\b')

# 0.0.13 patch 13h + 0.0.14 patch 14a + 0.0.20 patch 20a: top-N per
# set for the three-set picker. History:
#
# 13h originally used a 13% sum-cutoff. The 0.0.13 refresh empirically
# showed 6 of 9 measurable targets produced ZERO inter/public
# significance under that cutoff - long-tail workspaces dilute the
# sum-denominator. the_user 2026-06-03 follow-up: 'take the top 7
# for each set (and support that number with 13% as the guiding
# principle)' - 1 / 0.13 = 7.69, rounded to 7.
#
# 0.0.20 patch 20a: top_n becomes SLOC-scaled for workspace-wide
# sets (inter, public) so larger projects get more picks. Intra-per-
# crate stays at 7 because each crate is locally bounded. the_user
# 2026-06-04 directive: 'i'd rather slightly over-produce than
# under produce, because if we don't capture enough ... the knowledge
# product ends up requiring the agent to do code-reads anyway. 19
# for bevy gives us breathing room there'.
#
# Floor (7) preserved as the original 13% argument's anchor; scaling
# extends beyond when the workspace's architectural surface is
# empirically larger. Originally anchored to sourcetrait_common's
# raw line count (14K LoC) with 2.0 multiplier.
#
# 0.0.25 recalibration: input is now strict SLOC (no comments, no
# blanks, no test blocks per the_user 2026-06-04 'tests are meaningless
# to us'). sourcetrait_common drops to ~8.6K SLOC from ~14K LoC; other
# workspaces shrink by varying amounts based on test density.
# Calibrated (D=3100, M=1.6) via grid search across (D, M) - the
# parameters that meet-or-exceed ALL 0.0.24 caps (10 workspaces + 377
# per-crate entries) with minimum sum-of-rises. the_user 2026-06-04:
# 'log is the right tool for the job. run through variants of the
# formula and find one that meets or exceeds caps'.
#
# 0.0.27 recalibration: D dropped to 300, M held at 1.6. After 0.0.26
# landed pub_type entries (AST-derived), pattern_metrics grew (e.g.
# ratatui 423 -> 496) while picks stayed bounded by the prior cap.
# Coverage on the highest-covered repo (ratatui) dropped to 21.37%.
# the_user 2026-06-04: 'we have a lot of room ... start with hitting
# 25% or higher'. Grid search at the workspace level (10 targets, 7
# (D, M) candidates) ranked by sum_coverage subject to:
#   1. ratatui coverage in [25%, 29%]
#   2. no target coverage > 30% (per mem:rust-recon-coverage-cap-30pct)
#   3. no cap drops vs 0.0.25 (formula monotonic decreasing in D, so
#      lowering D never drops any workspace or per-crate cap)
# Picked (D=300, M=1.6): ratatui 26.41% (middle of band), all targets
# <30%, all caps rise. Aggregate measure_overlap holds at 97.5%; 7 of
# 8 measurable targets at 100% (only iced Update remains). Displaced
# 0.0.25 factory-call picks (EngineState::new, Selection::range,
# Buffer::with_lines, etc.) recover alongside the pub_type entries.
#
# 0.0.29 widening: D dropped to 100, M held at 1.6. After 0.0.28
# closed iced Update via method_ref family entries, the_user 2026-
# 06-04 asked for the higher-band calibration variables. The 0.0.27
# grid search had identified (D=100, M=1.6) as the highest-sum_cov
# candidate that still keeps all targets <30%: ratatui 28.23%,
# sum_cov 188.23. Single-parameter change from 0.0.27 (D 300 -> 100).
# All caps rise further (monotonicity preserved). Aggregate
# measure_overlap continues to hold at 100% (mechanical-only ceiling
# reached at 0.0.28; widening admits MORE picks per ground-truth
# entry without changing match counts).
# 0.0.30: moved to calibration.toml [picker]. Current values D=100,
# M=1.6 reflect 0.0.29's widening; the_user-tunable via TOML edit or
# env var override (ORIENT_SLOC_DIVISOR / ORIENT_SLOC_MULTIPLIER /
# ORIENT_TOP_N_FLOOR).
_TOP_N_FLOOR = config.int_param(
    "ORIENT_TOP_N_FLOOR", "picker", "top_n_floor", default=7)
_SLOC_DIVISOR = config.int_param(
    "ORIENT_SLOC_DIVISOR", "picker", "sloc_divisor", default=100)
_SLOC_MULTIPLIER = config.float_param(
    "ORIENT_SLOC_MULTIPLIER", "picker", "sloc_multiplier", default=1.6)


def _compute_sloc_scaled_top_n(sloc: int) -> int:
    """0.0.20 patch 20a + 0.0.25: compute the SLOC-scaled top_n for
    workspace-wide significance sets (architecture / public / inter-
    crate / internals) AND per-crate sets (intra-crate / inner-crate).

    Formula: max(_TOP_N_FLOOR, round(_TOP_N_FLOOR + _SLOC_MULTIPLIER *
    log2(sloc / _SLOC_DIVISOR))).

    0.0.25: input is now strict SLOC (no comments, no blanks, no
    test blocks), not raw line count. Divisor recalibrated to anchor
    sourcetrait_common at the floor. the_user 2026-06-04: 'we like
    those caps where they are. we just want to normalize'.

    For sloc below _SLOC_DIVISOR, returns the floor (7). For sloc
    >= _SLOC_DIVISOR, scales up. Each doubling adds _SLOC_MULTIPLIER
    picks.

    Target caps (preserved from 0.0.24):
    - sourcetrait_common -> 7 (floor)
    - libcosmic -> 11
    - helix -> 12
    - iced -> 14
    - nushell -> 18
    - bevy -> 19
    """
    import math
    if sloc <= 0:
        return _TOP_N_FLOOR
    ratio = sloc / _SLOC_DIVISOR
    if ratio < 1.0:
        return _TOP_N_FLOOR
    scaled = _TOP_N_FLOOR + _SLOC_MULTIPLIER * math.log2(ratio)
    return max(_TOP_N_FLOOR, round(scaled))

# 0.0.13 patch 13k + 0.0.21 patch 21a: examples contribute to PUBLIC
# SET ONLY. Each curated example (in /examples/ directory)
# contributes example_weight to the public set's count. the_user
# 2026-06-04 directive replaces the prior dual-weight (1.0 default /
# 1.5 serious-when-curated-count>=3) with log-scaled weight per
# workspace: 'use number of example rs files logarathmically to
# determine the weight applied to public category'. Workspaces with
# more example files signal stronger developer attention; the weight
# scales accordingly.
#
# Formula: max(_EXAMPLE_WEIGHT_FLOOR, log2(max(1, num_example_rs_files))).
# - 0-1 example files: 1.0 (floor)
# - 2: 1.0 (still at floor or just at log2)
# - 4: 2.0
# - 16: 4.0
# - 64: 6.0
# - 128 (bevy / iced scale): 7.0
_EXAMPLE_WEIGHT_FLOOR = config.float_param(
    "ORIENT_EXAMPLE_WEIGHT_FLOOR",
    "picker", "example", "weight_floor", default=1.0)

# 2026-06-05: 'internals' set replaced by 'clique' via STV (single
# transferable vote) over each crate's intra top-N as a ranked ballot.
# K = top_n_workspace seats, Droop quota. No threshold knob -- the
# quota arises naturally from the V/K ratio. See _stv_elect_clique
# below for the algorithm and per-target degeneracy notes.


def _compute_public_example_weight(num_example_rs_files: int) -> float:
    """0.0.21 patch 21a: log-scaled per-example weight for public set.

    Returns the per-example multiplier applied to curated_example_count
    in the public-set significance computation. Workspaces with more
    example .rs files get a larger multiplier, reflecting that heavy
    documentation-by-example signals stronger developer attention on
    the patterns those examples demonstrate.

    Floor at _EXAMPLE_WEIGHT_FLOOR (1.0) for workspaces with 0 or 1
    example file; otherwise log2 of the file count. Reaches 7.0 at
    128 example files (bevy / iced scale)."""
    import math
    if num_example_rs_files <= 1:
        return _EXAMPLE_WEIGHT_FLOOR
    return max(_EXAMPLE_WEIGHT_FLOOR, math.log2(num_example_rs_files))


# 0.0.10 patch 10f: combined-score ranking constants. Score formula:
#   score = raw_count
#         * (1 + alpha * inter_ratio)
#         * (1 + beta * is_pub)
#         * (1 + gamma * min(example_count, _EXAMPLE_SATURATION) / _EXAMPLE_SATURATION)
#         * (delta if example_count >= _EXAMPLE_THRESHOLD else 1)
# alpha = inter-crate flow boost; beta = public-API boost; gamma =
# example-presence boost (saturating at _EXAMPLE_SATURATION examples);
# delta = developer-signaled-importance boost when example count
# exceeds _EXAMPLE_THRESHOLD (the_user 2026-06-03: 'more than 3 examples
# exist' = developers signaling strong usage guidelines).
_SCORE_INTER_BOOST = config.float_param(
    "ORIENT_SCORE_INTER_BOOST",
    "picker", "score", "inter_boost", default=0.5)
_SCORE_PUB_BOOST = config.float_param(
    "ORIENT_SCORE_PUB_BOOST",
    "picker", "score", "pub_boost", default=0.3)
_SCORE_EXAMPLE_BOOST = config.float_param(
    "ORIENT_SCORE_EXAMPLE_BOOST",
    "picker", "score", "example_boost", default=0.5)
_SCORE_EXAMPLE_THRESHOLD_BOOST = config.float_param(
    "ORIENT_SCORE_EXAMPLE_THRESHOLD_BOOST",
    "picker", "score", "example_threshold_boost", default=1.5)
_EXAMPLE_SATURATION = config.int_param(
    "ORIENT_EXAMPLE_SATURATION",
    "picker", "example", "saturation", default=10)
_EXAMPLE_THRESHOLD = config.int_param(
    "ORIENT_EXAMPLE_THRESHOLD",
    "picker", "example", "threshold", default=3)


def _compute_score(count, metrics):
    """0.0.10 patch 10f: centrality score for a pattern. count is the
    raw per-crate or workspace-wide count; metrics is the pattern_metrics
    entry for the pattern (or aggregated metrics for families). Returns
    a float; higher = more architecturally central.

    Missing metrics fields default to neutral values (inter_ratio=0,
    is_pub=False, example_count=0). External patterns with no metrics
    surface at raw count.

    0.0.12 patch 12d: example_count is now the weighted total
    (examples + 0.3 * tests + 0.3 * benches by default). The threshold
    boost uses curated_example_count (examples/-only) so the
    'more than 3 examples exist' developer-signal semantics stays
    strict per the_user's framing while the saturation boost
    captures the broader pattern-importance signal that tests contribute.
    """
    if not metrics:
        return float(count)
    inter_ratio = metrics.get("inter_ratio", 0.0) or 0.0
    is_pub = 1.0 if metrics.get("is_pub", False) else 0.0
    example_count = metrics.get("example_count", 0) or 0
    curated_example_count = metrics.get("curated_example_count", 0) or 0
    score = float(count)
    score *= 1.0 + _SCORE_INTER_BOOST * inter_ratio
    score *= 1.0 + _SCORE_PUB_BOOST * is_pub
    sat = min(example_count, _EXAMPLE_SATURATION) / max(1, _EXAMPLE_SATURATION)
    score *= 1.0 + _SCORE_EXAMPLE_BOOST * sat
    if curated_example_count >= _EXAMPLE_THRESHOLD:
        score *= _SCORE_EXAMPLE_THRESHOLD_BOOST
    return score


# 0.0.4 patch 4 (u): when the workspace has more than _CLUSTER_THRESHOLD crates, the
# S1 crate / region map emits a "Crate clusters (by name prefix)" sub-section above
# the per-crate detail list. Clusters require at least _CLUSTER_MIN_SIZE members.
_CLUSTER_THRESHOLD = config.int_param(
    "ORIENT_CLUSTER_THRESHOLD",
    "picker", "cluster", "threshold", default=15)
_CLUSTER_MIN_SIZE = config.int_param(
    "ORIENT_CLUSTER_MIN_SIZE",
    "picker", "cluster", "min_size", default=3)


def _cluster_crates_by_prefix(crate_names):
    """Group crate names by longest shared name prefix (split on '_' or '-' independently).

    Returns (clusters, others) where clusters is {prefix: [names]} for prefixes with at
    least _CLUSTER_MIN_SIZE members, and others is the leftover standalone-crate list.
    Snake-case and kebab-case prefixes are kept DISTINCT (nu-plugin vs nu_plugin) since
    a workspace can use both conventions for different roles - nushell uses kebab for
    SDK / runtime crates (nu-plugin-*) and snake for bundled plugin binaries
    (nu_plugin_*); these are different architectural clusters."""
    crates = sorted(crate_names)

    def prefix_candidates(name):
        # Returns [longest, ..., shortest] prefix candidates excluding the full name.
        # Snake and kebab are tried independently; results are deduped while preserving
        # longest-first order.
        results = []
        for sep in ('_', '-'):
            if sep not in name:
                continue
            parts = name.split(sep)
            for k in range(len(parts) - 1, 0, -1):
                p = sep.join(parts[:k])
                if p and p not in results:
                    results.append(p)
        return results

    prefix_members = defaultdict(set)
    for name in crates:
        for p in prefix_candidates(name):
            prefix_members[p].add(name)

    clusters = defaultdict(list)
    others = []
    for name in crates:
        assigned = None
        for p in prefix_candidates(name):
            if len(prefix_members[p]) >= _CLUSTER_MIN_SIZE:
                assigned = p
                break
        if assigned:
            clusters[assigned].append(name)
        else:
            others.append(name)
    return dict(clusters), others


def sp(rec):
    """file:line(-end) span string."""
    f = rec.get("file", "?")
    ln = rec.get("line", "?")
    end = rec.get("end_line")
    return f"{f}:{ln}-{end}" if end and end != ln else f"{f}:{ln}"


def provenance(root: Path, odir: Path, fp: dict):
    commit = "UNKNOWN"
    rustc = "UNKNOWN"
    try:
        commit = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                                capture_output=True, text=True, timeout=10).stdout.strip() or "UNKNOWN"
    except Exception:
        pass
    try:
        rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True,
                               timeout=10).stdout.strip() or "UNKNOWN"
    except Exception:
        pass
    overlay = (odir / "rustdoc_overlay.json").exists()
    return (f"commit: {commit}\nrustc: {rustc}\ntool_version: {fp.get('tool_version')}\n"
            f"rustdoc_overlay_present: {overlay}")


def emit_reference(root: Path, fp: dict, facts: dict, out: Path):
    by_crate = defaultdict(lambda: {"types": [], "traits": [], "impls": [], "fns": [],
                                    "macros": [], "reexports": []})
    for t in facts["types"]:
        by_crate[t.get("crate", "?")]["types"].append(t)
    for t in facts["traits"]:
        by_crate[t.get("crate", "?")]["traits"].append(t)
    for i in facts["impls"]:
        by_crate[i.get("crate", "?")]["impls"].append(i)
    for f in facts["fns"]:
        if f.get("brace_depth") == 0:  # free functions only in the index headline
            by_crate[f.get("crate", "?")]["fns"].append(f)
    for m in facts["macros"]:
        by_crate[m.get("crate", "?")]["macros"].append(m)
    for u in facts["uses"]:
        if u.get("reexport"):
            by_crate[u.get("crate", "?")]["reexports"].append(u)

    L = ["# Reference Index", "",
         "Exhaustive, span-anchored. Grep this; do not read it top to bottom. Every entry is",
         "a `file:line` you open in source to verify or extend. Spans the rustdoc overlay",
         "marks null (re-exports, blanket/synthesized/macro-generated impls) are flagged, not",
         "dropped.", "", "```", provenance(root, out.parent, fp), "```", ""]
    for crate in sorted(by_crate):
        c = by_crate[crate]
        L.append(f"## crate: {crate}")
        if c["traits"]:
            L.append("### traits")
            for t in sorted(c["traits"], key=lambda x: x["name"]):
                cfg = "  *(cfg-gated)*" if t.get("cfg_gated") else ""
                L.append(f"- `{t['name']}` - {sp(t)}{cfg}")
        if c["types"]:
            L.append("### types")
            for t in sorted(c["types"], key=lambda x: x["name"]):
                cfg = "  *(cfg-gated)*" if t.get("cfg_gated") else ""
                L.append(f"- `{t['kind']} {t['name']}` - {sp(t)}{cfg}")
        if c["impls"]:
            L.append("### impls")
            for i in sorted(c["impls"], key=lambda x: (str(x.get("trait")), str(x.get("type")))):
                tr = f"`{i['trait']}` for " if i.get("trait") else "(inherent) "
                cfg = "  *(cfg-gated)*" if i.get("cfg_gated") else ""
                L.append(f"- impl {tr}`{i.get('type')}` - {sp(i)}{cfg}")
        if c["fns"]:
            L.append("### free functions")
            for f in sorted(c["fns"], key=lambda x: x["name"]):
                L.append(f"- `fn {f['name']}` - {sp(f)}")
        if c["macros"]:
            L.append("### macro applications *(expansion unverified without rustdoc overlay)*")
            for m in sorted(c["macros"], key=lambda x: x.get("line", 0)):
                if m["kind"] == "macro_invocation":
                    args = ", ".join(m.get("arg_idents", [])[:12])
                    L.append(f"- `{m['name']}!(...)` args=[{args}] - {sp(m)}")
                else:
                    L.append(f"- `#[{m['name']}]` - {sp(m)}")
        if c["reexports"]:
            L.append("### re-exports *(rustdoc resolves the target; span may be null)*")
            for u in c["reexports"]:
                L.append(f"- `{u['path']}` - {sp(u)}")
        L.append("")
    out.write_text("\n".join(L))


def core_vocabulary(fp: dict, facts: dict):
    """Heuristic: the core types live in the most-depended-on crate. Prefer in-workspace
    crates (those that appear in fp["per_crate"] keys) over external infra crates - the S2
    vocabulary should be the domain language other crates in the workspace speak in, not a
    shared error-helper or utility crate from crates.io. Falls back to the global most-
    depended-on pick only when no in-workspace crate has any dependents at all (unusual).

    0.0.4 patch 2 (s): within the picked crate, traits and types are RANKED BY USAGE
    (impl-block references) descending, with alphabetical-by-name as the tiebreaker. The
    prior 0.0.3 behavior was alphabetical, which alphabetically-truncated the top-40 list
    at A-through-C on big crates; the load-bearing nu-protocol types (Value, PipelineData,
    Span, EngineState, Stack, Signature, IrBlock, etc.) all fell past the cutoff. Usage
    ranking surfaces them instead. Each returned item is annotated with `_usage` (an int)
    for emit_orientation to render alongside the entry."""
    dep_count = defaultdict(int)
    for c in fp["per_crate"].values():
        for d in c["deps"]:
            dep_count[d] += 1
    in_workspace = {k: v for k, v in dep_count.items() if k in fp["per_crate"]}
    pick_pool = in_workspace if in_workspace else dep_count
    core = max(pick_pool, key=pick_pool.get) if pick_pool else None
    # Filter types/traits to src/ only - the 0.0.2 #5 sweep partition for S3 seam sites,
    # now extended to S2 vocab so test-file types (ratatui/tests/*.rs, tokio/tests/*.rs)
    # do not pollute the listed core vocabulary.
    types = [t for t in facts["types"]
             if t.get("crate") == core and _is_src_file(t.get("file", ""))]
    traits = [t for t in facts["traits"]
              if t.get("crate") == core and _is_src_file(t.get("file", ""))]
    # 0.0.4 patch 2 (s): rank by impl-block usage (trait_usage) AND use-statement
    # occurrence (type_usage augmented with `use` references). Trait usage = count of
    # `impl <T> for ...` blocks referencing the trait. Type usage starts at the count of
    # `impl ... for <T>` impl-target appearances and is augmented by each `use ...::<T>`
    # statement where the path's tail equals <T>. Generic args and module prefix are
    # stripped so `Vec<T>` / `crate::foo::Vec` / `use foo::Vec` all contribute to bare
    # `Vec`'s count. Catches inherent-method types (Span, Stack, Signature) that have few
    # impl-targets but many cross-crate `use` references.
    trait_usage = Counter()
    type_usage = Counter()
    for i in facts["impls"]:
        if i.get("trait"):
            trait_usage[i["trait"]] += 1
        if i.get("type"):
            t_str = str(i["type"])
            bare = t_str.split("<", 1)[0].split("::")[-1].strip()
            if bare:
                type_usage[bare] += 1
    # Augment type_usage with `use` path occurrences. Rustscan captures the full path
    # including braced multi-item imports (e.g.
    # `use nu_protocol::{Value, Span, EngineState, Stack, Signature}` is ONE path string
    # with embedded braces and newlines). We pull every word-bounded capitalized
    # identifier out of the path and count each as a use-occurrence; this picks up
    # multi-item brace imports correctly.
    for u in facts.get("uses", []):
        path = u.get("path", "")
        if not path:
            continue
        for ident in _TYPE_IDENT_RE.findall(path):
            type_usage[ident] += 1
    for t in traits:
        t["_usage"] = trait_usage.get(t["name"], 0)
    for t in types:
        t["_usage"] = type_usage.get(t["name"], 0)
    traits.sort(key=lambda t: (-t["_usage"], t["name"]))
    types.sort(key=lambda t: (-t["_usage"], t["name"]))
    return core, types, traits


def _instance_for_kind(kind, name, facts):
    """Pick a sample instance for a pattern of the given (kind, name). Returns
    (instance, all_spans) where instance is None if no actionable item exists for
    that kind. Used by candidate_instance both for the histogram leader and (0.0.4
    patch 5) for the load-bearing fallback walk."""
    if kind == "trait_impl":
        inst = [i for i in facts["impls"]
                if i.get("trait") == name and not i.get("cfg_gated")]
        inst.sort(key=lambda x: str(x.get("type")))
        return (inst[0] if inst else None, [sp(i) for i in inst[:200]])
    if kind == "derive":
        inst = [d for d in facts["derives"] if d.get("trait") == name]
        return (inst[0] if inst else None,
                [f"{d.get('file','?')}:{d['line']}" for d in inst[:200]])
    if kind == "reg_macro":
        inst = [m for m in facts["macros"]
                if m["kind"] == "macro_invocation" and m["name"] == name]
        return (inst[0] if inst else None, [sp(m) for m in inst[:200]])
    if kind == "type_usage":
        # 0.0.8 patch 8c: type_usage instances are call-site dicts from
        # facts['type_usages'] matched by combined "<outer>::<inner>" name.
        # 0.0.11 patch 11a: fall back to example_type_usages when src has
        # none. Patterns that exist only in tests/examples (tokio's
        # mpsc::channel: 0 src + 80 example calls) need their instance
        # to come from the example pool, not nowhere.
        inst = [tu for tu in facts.get("type_usages", [])
                if tu.get("name") == name]
        if not inst:
            inst = [tu for tu in facts.get("example_type_usages", [])
                    if tu.get("name") == name]
        return (inst[0] if inst else None,
                [f"{tu.get('file','?')}:{tu['line']}" for tu in inst[:200]])
    if kind == "method_ref":
        # 0.0.28: method_ref pattern names are FAMILY entries shaped
        # as "_::<inner>" (placeholder outer per family aggregation;
        # each entry represents the method name as architectural
        # protagonist across N workspace-defined-pub outers). Instance
        # lookup matches by inner: any ast_method_refs entry whose
        # inner equals the family's inner is a valid seed.
        family_inner = name.split("::", 1)[1] if "::" in name else name
        inst = [r for r in facts.get("ast_method_refs", [])
                if r.get("inner") == family_inner]
        return (inst[0] if inst else None,
                [f"{r.get('file','?')}:{r['line']}" for r in inst[:200]])
    if kind == "pub_type":
        # 0.0.26: pub_type instance is the type's or trait's
        # definition site from facts.types / facts.traits.
        # Workspace-defined pub items synthesized in
        # _compute_pattern_metrics; instance points to the def.
        inst = [t for t in facts.get("types", [])
                if t.get("name") == name]
        if not inst:
            inst = [t for t in facts.get("traits", [])
                    if t.get("name") == name]
        return (inst[0] if inst else None,
                [f"{t.get('file','?')}:{t['line']}" for t in inst[:200]])
    if kind == "type_usage_family":
        # 0.0.9 patch 9a: family instances aggregate call sites from all
        # matching type_usages. outer:X matches names starting with X::;
        # inner:Y matches names ending with ::Y.
        # 0.0.11 patch 11a: same example_type_usages fallback as singular
        # type_usage. Families derived from example-only patterns
        # (AppBuilder in helix tests) need their instance from the example
        # pool. Without this, the picker selects the family then drops it
        # at instance lookup, leaving the slot unfilled.
        if name.startswith("outer:"):
            outer = name.split(":", 1)[1]
            inst = [tu for tu in facts.get("type_usages", [])
                    if tu.get("name", "").split("::", 1)[0] == outer]
            if not inst:
                inst = [tu for tu in facts.get("example_type_usages", [])
                        if tu.get("name", "").split("::", 1)[0] == outer]
        elif name.startswith("inner:"):
            inner = name.split(":", 1)[1]
            inst = [tu for tu in facts.get("type_usages", [])
                    if tu.get("name", "").rpartition("::")[2] == inner]
            if not inst:
                inst = [tu for tu in facts.get("example_type_usages", [])
                        if tu.get("name", "").rpartition("::")[2] == inner]
        else:
            inst = []
        return (inst[0] if inst else None,
                [f"{tu.get('file','?')}:{tu['line']}" for tu in inst[:200]])
    return (None, [])


def _stv_elect_clique(per_crate_ballots,
                      num_seats,
                      dedup_keys=None):
    """Single Transferable Vote (STV) election to elect `num_seats`
    'clique' patterns from per-crate ranked ballots.

    Each workspace crate is a voter; its ballot is its intra top-N
    ordered by usage count (highest first). Each ballot starts at
    weight 1.0. Standard fractional-Droop STV:

    1. Quota Q = V / (K + 1), where V = ballot count, K = num_seats.
    2. Tally each ballot's current preference (skipping already
       elected or eliminated candidates).
    3. Any candidate at-or-above quota is elected. Their surplus
       (votes - Q) transfers fractionally to next preferences via a
       weight adjustment on every supporting ballot.
    4. If no candidate is at quota, eliminate the lowest (alphabetical
       tie-break) and let supporting ballots flow to next preferences.
    5. Loop until K seats filled or no eligible preferences remain.

    `dedup_keys` (optional): set of patterns already elected in OTHER
    workspace-wide sets (architecture / public / inter_crate). These
    are pre-stripped from each ballot so seats aren't wasted on
    duplicates.

    Returns dict {pattern: vote_total_at_election}, in election
    order (Python dict insertion).

    Design (the_user 2026-06-05): replaces the sum-of-counts +
    spread-threshold 'internals' heuristic so each crate has equal
    voice (no domination by one heavy-user crate). Degenerate cases
    (V <= K, quota collapses to ~1) are accepted as honest output -
    a small workspace has no real clique to discover.
    """
    if dedup_keys is None:
        dedup_keys = set()

    ballots = []
    for crate, ranked in per_crate_ballots.items():
        clean = [p for p in ranked if p not in dedup_keys]
        if clean:
            ballots.append(clean)

    V = len(ballots)
    K = num_seats
    if V == 0 or K == 0:
        return {}

    Q = V / (K + 1)

    weights = [1.0] * V
    pointers = [0] * V
    elected = {}
    eliminated = set()

    def _current(i):
        while pointers[i] < len(ballots[i]):
            p = ballots[i][pointers[i]]
            if p in elected or p in eliminated:
                pointers[i] += 1
            else:
                return p
        return None

    while len(elected) < K:
        tally = defaultdict(float)
        supporters = defaultdict(list)
        for i in range(V):
            if weights[i] <= 0:
                continue
            p = _current(i)
            if p is not None:
                tally[p] += weights[i]
                supporters[p].append(i)

        if not tally:
            break

        over_quota = sorted(
            ((c, v) for c, v in tally.items() if v >= Q),
            key=lambda x: (-x[1], x[0]),
        )

        if over_quota:
            for c, votes in over_quota:
                if len(elected) >= K:
                    break
                elected[c] = votes
                surplus = votes - Q
                transfer_factor = (surplus / votes) if votes > 0 else 0.0
                for i in supporters[c]:
                    weights[i] *= transfer_factor
        else:
            min_pattern = min(tally.items(), key=lambda x: (x[1], x[0]))[0]
            eliminated.add(min_pattern)

    return elected


def _compute_significance_sets(fp: dict, facts: dict,
                               per_crate_sloc: dict = None,
                               top_n_workspace: int = None):
    """0.0.13 patch 13h + 0.0.14 patch 14a + 0.0.20 patch 20a + 0.0.22
    + 0.0.23 + 0.0.24: six-set significance picker.

    0.0.22 named the doubly-strong cross-crate+public set 'architecture'
    + restructured per-crate sets into strict-origin split (intra_crate
    = defining_crate != crate; inner_crate = defining_crate == crate).
    0.0.23 conformed naming + dropped unused picks scaffolding.
    0.0.24 adds 'internals' (workspace-wide cross-crate-spread set:
    intersection of >= _INTERNALS_SPREAD_THRESHOLD crates' initial
    intra top-N; the_user 2026-06-04: 'one last category: internals ->
    intersection of highest ranked intra_crate items') + iterates
    intra_crate to dedup against (workspace-wide ∪ internals).

    Returns dict with:
    - significant_architecture: {pattern: combined_score}
        top top_n_workspace patterns scoring on BOTH cross-crate
        flow AND public-by-example signals (the doubly-strong set).
    - significant_public: {pattern: public_score}
        top top_n_workspace patterns by curated_example_count *
        public_example_weight, is_pub only.
    - significant_inter_crate: {pattern: inter_count}
        top top_n_workspace patterns by inter_count workspace-wide.
    - significant_internals: {pattern: total_count_across_crates}
        top top_n_workspace patterns appearing in >= 2 crates'
        initial intra top-N (workspace-wide cross-crate spread).
    - significant_intra_crate_per_crate: {crate: {pattern: count}}
        per-crate top-N where defining_crate != crate; second-pass
        dedup against (workspace-wide ∪ internals), walk-deeper.
    - significant_inner_crate_per_crate: {crate: {pattern: count}}
        per-crate top-N where defining_crate == crate (own-origin).

    top_n_workspace defaults to _TOP_N_FLOOR; callers should compute
    the SLOC-scaled value via _compute_sloc_scaled_top_n and pass it.
    Per-crate top-N scaled per crate via per_crate_sloc lookup."""
    if top_n_workspace is None:
        top_n_workspace = _TOP_N_FLOOR
    per_crate_sloc = per_crate_sloc or {}
    pattern_metrics = fp.get("pattern_metrics", {})

    def _top_n_for_crate(crate: str) -> int:
        return _compute_sloc_scaled_top_n(per_crate_sloc.get(crate, 0))

    # 0.0.20 patch 20b: filter per_crate_counts to workspace-originated
    # patterns only. the_user 2026-06-04: 'drop std results entirely in
    # our scripts. it needs to be originated code, not usage of external
    # libraries'. Same filter the inter / public sets already apply
    # (defining_crate is not None means the trait / type / macro is
    # declared in a workspace crate). Without this filter, intra picks
    # were dominated by std-derived patterns (derive:Debug / Clone /
    # PartialEq / Default / Copy / etc.) that are universally derived
    # for utility rather than architectural insight.
    def _is_workspace_originated(pattern: str) -> bool:
        m = pattern_metrics.get(pattern, {})
        return m.get("defining_crate") is not None

    # Build per-crate counts across all pattern kinds.
    per_crate_counts = defaultdict(lambda: defaultdict(int))
    for it in facts.get("impls", []):
        if it.get("trait") and not it.get("cfg_gated"):
            c = it.get("crate")
            if c:
                p = f"trait_impl:{it['trait']}"
                if _is_workspace_originated(p):
                    per_crate_counts[c][p] += 1
    for d in facts.get("derives", []):
        c = d.get("crate")
        nm = d.get("trait")
        if c and nm:
            p = f"derive:{nm}"
            if _is_workspace_originated(p):
                per_crate_counts[c][p] += 1
    for tu in facts.get("type_usages", []):
        c = tu.get("crate")
        nm = tu.get("name")
        if c and nm:
            p = f"type_usage:{nm}"
            if _is_workspace_originated(p):
                per_crate_counts[c][p] += 1
    # 0.0.26: AST-derived pub_type entries deliberately NOT added to
    # per_crate_counts. The_user 2026-06-04 cost-of-displacement
    # lesson: bumping pub_type counts into the per-crate sets crowds
    # out trait_impl entries (helix's trait_impl:Component dropped
    # when pub_type:Context with 503 hits muscled in). pub_type
    # entries serve workspace-wide sets (architecture / public /
    # inter-crate / internals) via pattern_metrics; per-crate sets
    # stay backed by the original facts data.
    for m in facts.get("macros", []):
        c = m.get("crate")
        kind = m.get("kind")
        nm = m.get("name")
        if c and nm:
            if kind == "macro_invocation":
                p = f"reg_macro:{nm}"
                if _is_workspace_originated(p):
                    per_crate_counts[c][p] += 1
            elif kind == "attr_macro":
                p = f"attr_macro:{nm}"
                if _is_workspace_originated(p):
                    per_crate_counts[c][p] += 1

    # 0.0.22 reorder: workspace-wide sets are computed FIRST; per-crate
    # intra + inner-crate sets dedup against them. Strict-origin split
    # for intra/inner is applied below the workspace block.

    # 1. Workspace-wide significance partitioned into THREE
    # disjoint sets per the_user 2026-06-04: 'create one more category
    # that is a union of inter-crate and public where they share, which
    # would remove slots from both inter-crate and public'. 0.0.22
    # rename: the doubly-strong category is "architecture"
    # (the_user 2026-06-04: 'intersection is too vague').
    #
    # Raw signals:
    # - Inter-crate signal (cross-crate flow): inter_count.
    # - Public signal (docs-by-example): curated_example_count * weight.
    #
    # Partition:
    # - architecture: pattern has BOTH signals (is_pub + curated > 0 +
    #   inter_count > 0). Doubly-strong architectural protagonists.
    # - inter-crate: inter_count > 0 but not in architecture (no curated
    #   example signal or not is_pub).
    # - public: curated * weight > 0 but not in architecture
    #   (no cross-crate flow).
    #
    # The architecture set takes slots from both inter-crate and public
    # so the agent's reading doesn't waste slots on duplication.
    num_example_rs_files = fp.get("totals", {}).get("example_rs_files", 0)
    public_example_weight = _compute_public_example_weight(num_example_rs_files)

    # Build raw-signal scores per pattern.
    public_scores = {}  # curated * weight (pure docs signal)
    inter_scores = {}   # inter_count (pure cross-crate-flow signal)
    for pattern, m in pattern_metrics.items():
        if m.get("defining_crate") is None:
            continue
        ic = m.get("inter_count", 0) or 0
        curated = m.get("curated_example_count", 0) or 0
        is_pub = bool(m.get("is_pub"))
        if is_pub and curated > 0:
            public_scores[pattern] = curated * public_example_weight
        if ic > 0:
            inter_scores[pattern] = ic

    # Architecture: patterns with both signals.
    architecture_keys = set(public_scores) & set(inter_scores)
    architecture_counts = {
        p: public_scores[p] + inter_scores[p]
        for p in architecture_keys
    }

    # Inter-crate and public: residuals after architecture removal.
    inter_counts = {
        p: s for p, s in inter_scores.items() if p not in architecture_keys
    }
    public_counts = {
        p: s for p, s in public_scores.items() if p not in architecture_keys
    }

    # Top N per disjoint set.
    significant_inter_crate = {}
    if inter_counts:
        sorted_inter = sorted(inter_counts.items(), key=lambda x: -x[1])
        significant_inter_crate = dict(sorted_inter[:top_n_workspace])
    significant_public = {}
    if public_counts:
        sorted_public = sorted(public_counts.items(), key=lambda x: -x[1])
        significant_public = dict(sorted_public[:top_n_workspace])
    significant_architecture = {}
    if architecture_counts:
        sorted_isect = sorted(architecture_counts.items(),
                              key=lambda x: -x[1])
        significant_architecture = dict(sorted_isect[:top_n_workspace])

    # 2 + 3 + 4. Per-crate intra + inner-crate (strict origin) +
    # workspace-wide internals (intersection of multi-crate intra
    # picks).
    #
    # the_user 2026-06-04: 'intra-crate should be reserved for usage
    # of other crates' + 'inner-crate: source-code item originates in
    # the crate and is called by the crate' + 'internals: intersection
    # of highest ranked intra_crate items'. Strict origin split makes
    # intra + inner disjoint by construction:
    # - intra-X = patterns USED in X with defining_crate != X
    # - inner-X = patterns USED in X with defining_crate == X
    #
    # Internals captures workspace-wide cross-crate spread: patterns
    # appearing in MULTIPLE crates' intra top-N (after initial dedup
    # vs workspace-wide). Two-pass intra computation:
    # - Pass 1: initial intra_crate (dedup vs workspace-wide).
    # - Build internals from spread >= threshold of pass-1 picks;
    #   score = sum of per-crate counts; cap at top_n_workspace.
    # - Pass 2: re-compute intra_crate (dedup vs workspace-wide ∪
    #   internals), walk deeper to fill per-crate-LoC-scaled top-N.
    #
    # inner-crate is independent of internals (origin == crate
    # cannot have cross-crate spread by construction); still single-
    # pass dedup vs workspace-wide.
    workspace_wide_keys = (
        set(significant_architecture)
        | set(significant_inter_crate)
        | set(significant_public)
    )

    def _per_crate_picks(origin_match, dedup_keys):
        """Return (sig_dict, top_n_dict). origin_match: True means
        defining_crate == crate (inner-crate); False means
        defining_crate != crate (intra-crate, strict-origin).
        dedup_keys: patterns to skip (already covered elsewhere)."""
        sig_per_crate = {}
        top_n_per_crate = {}
        for crate, counts in per_crate_counts.items():
            if not counts:
                continue
            top_n = _top_n_for_crate(crate)
            top_n_per_crate[crate] = top_n
            filtered = []
            for p, c in counts.items():
                if p in dedup_keys:
                    continue
                defining = pattern_metrics.get(p, {}).get("defining_crate")
                if origin_match and defining != crate:
                    continue
                if (not origin_match) and (defining is None or defining == crate):
                    continue
                filtered.append((p, c))
            if not filtered:
                continue
            filtered.sort(key=lambda x: -x[1])
            sig = dict(filtered[:top_n])
            if sig:
                sig_per_crate[crate] = sig
        return sig_per_crate, top_n_per_crate

    # Pass 1: initial intra_crate (dedup vs workspace-wide).
    initial_intra_per_crate, _ = _per_crate_picks(
        origin_match=False, dedup_keys=workspace_wide_keys)

    # Clique (2026-06-05, replaces internals): STV election with each
    # crate's intra top-N as a ranked ballot. Equal voting power per
    # crate avoids the sum-of-counts pathology where one heavy-user
    # crate dominates. K = top_n_workspace seats; Droop quota; pre-
    # deduped against the prior workspace-wide sets.
    per_crate_ballots = {
        crate: list(sig.keys())  # already ordered by count (top-N sorted)
        for crate, sig in initial_intra_per_crate.items()
    }
    significant_clique = _stv_elect_clique(
        per_crate_ballots,
        num_seats=top_n_workspace,
        dedup_keys=workspace_wide_keys,
    )

    # Clique IS workspace-wide -- fold it into workspace_wide_keys so the
    # downstream per-crate dedup uses one unified set (the_user 2026-06-
    # 05: 'technically clique is a workspace_wide set').
    workspace_wide_keys |= set(significant_clique)

    # Pass 2: intra_crate dedup vs full workspace-wide (architecture,
    # public, inter_crate, clique); walk deeper to fill freed slots.
    significant_intra_crate_per_crate, top_n_intra_crate_per_crate = _per_crate_picks(
        origin_match=False, dedup_keys=workspace_wide_keys)

    # inner-crate: dedup vs full workspace-wide so each pattern appears
    # in only one section across the orientation (the_user 2026-06-05:
    # prior design left inner-crate undeduped vs internals, so workspace-
    # shared patterns defined IN a crate showed up in BOTH 5.4 and 5.6;
    # now they appear only in 5.4).
    significant_inner_crate_per_crate, top_n_inner_per_crate = _per_crate_picks(
        origin_match=True, dedup_keys=workspace_wide_keys)

    return {
        "significant_intra_crate_per_crate": {
            c: dict(s) for c, s in significant_intra_crate_per_crate.items()
        },
        "significant_inner_crate_per_crate": {
            c: dict(s) for c, s in significant_inner_crate_per_crate.items()
        },
        "significant_inter_crate": dict(significant_inter_crate),
        "significant_public": dict(significant_public),
        "significant_architecture": dict(significant_architecture),
        "significant_clique": dict(significant_clique),
        "top_n_intra_crate_per_crate": top_n_intra_crate_per_crate,
        "top_n_inner_per_crate": top_n_inner_per_crate,
        "top_n_workspace": top_n_workspace,
    }


def candidate_instances(fp: dict, facts: dict):
    """Pick a list of architectural-pattern protagonists with seed instances.

    0.0.4 patch 5 (v): when the raw histogram leader is a kind-only signal that does
    not yield a structurally followable instance (fn_table:<crate> counts free
    functions, attr_macro:<external> may be an external derive surface, etc.), walk
    DOWN the histogram for the first trait_impl / derive / reg_macro entry that has
    a valid instance and surface THAT as the load-bearing pick.

    0.0.5 patch dd: the kind-only treatment is extended to generic-auto-derive
    leaders (Debug, Clone, PartialEq, Eq, Hash, Default, Copy, Serialize,
    Deserialize). These are universally applied via #[derive(...)] and are not
    architecturally load-bearing; picking one as the protagonist gives a generic
    trace that does not reveal the workspace's structural pattern. helix (rank-1
    leader was derive:Debug at 849) and bevy (rank-1 was derive:Clone at 3911)
    surfaced this; both should now pick non-generic alternatives. The fallback walk
    also skips generic-auto-derive entries entirely.

    0.0.6 patch 6b: workspace-defined-trait preference. The walk now runs in two
    passes - first preferring patterns where the trait or macro is workspace-defined
    (the workspace's architectural pattern lives there), then falling back to
    non-workspace patterns if no workspace-defined alternative exists. The leader
    is also held to the workspace check: if it's workspace-defined + non-generic +
    has an instance, it's returned directly; otherwise the walk takes over.

    0.0.7 patch 7a: return shape changes from `dict | None` to `list[dict]` (empty
    list when no candidates). This is a pure-refactor preparation for 7b's
    per-crate top-pattern aggregation - the function now CAN return multiple
    protagonists (single-item list initially to match the prior single-pick
    behavior, then 7b extends to genuine plurality). The single returned dict
    matches the prior shape exactly; emit_orientation accesses cands[0] for the
    legacy single-protagonist render.

    Each returned dict carries: kind, pattern, instance, all_spans,
    fallback_reason. 7b will add: source_crate (which crate's aggregation
    surfaced this pick)."""
    if not fp["pattern_histogram"]:
        return []
    # 0.0.13 + 0.0.22 + 0.0.25: significance-based set picker
    # (architecture / public / inter-crate / internals / intra-crate /
    # inner-crate). Workspace-wide top_n SLOC-scaled; per-crate intra +
    # inner top_n scale per-crate via _compute_sloc_scaled_top_n(
    # per_crate_sloc[crate]). 0.0.25 input is normalized SLOC (no
    # comments / blanks / test blocks).
    workspace_sloc = fp.get("totals", {}).get("sloc", 0)
    top_n_workspace = _compute_sloc_scaled_top_n(workspace_sloc)
    per_crate_sloc = {
        crate: info.get("sloc", 0)
        for crate, info in fp.get("per_crate", {}).items()
    }
    sig = _compute_significance_sets(
        fp, facts,
        per_crate_sloc=per_crate_sloc,
        top_n_workspace=top_n_workspace,
    )
    # Enrich each set with instance + spans per pattern.
    def _enrich(pattern_map):
        out = {}
        for pattern, count in pattern_map.items():
            kind, _, name = pattern.partition(":")
            inst, spans = _instance_for_kind(kind, name, facts)
            if inst is None:
                continue
            out[pattern] = {
                "kind": kind,
                "pattern": pattern,
                "count": count,
                "instance": inst,
                "all_spans": spans,
            }
        return out

    enriched_intra_crate_per_crate = {
        crate: _enrich(s) for crate, s in
        sig["significant_intra_crate_per_crate"].items()
    }
    enriched_inner_crate_per_crate = {
        crate: _enrich(s) for crate, s in
        sig["significant_inner_crate_per_crate"].items()
    }
    enriched_inter_crate = _enrich(sig["significant_inter_crate"])
    enriched_public = _enrich(sig["significant_public"])
    enriched_architecture = _enrich(sig.get("significant_architecture", {}))
    enriched_clique = _enrich(sig.get("significant_clique", {}))
    return {
        "intra_crate_per_crate": enriched_intra_crate_per_crate,
        "inner_crate_per_crate": enriched_inner_crate_per_crate,
        "inter_crate": enriched_inter_crate,
        "public": enriched_public,
        "architecture": enriched_architecture,
        "clique": enriched_clique,
        "top_n_intra_crate_per_crate": sig["top_n_intra_crate_per_crate"],
        "top_n_inner_per_crate": sig["top_n_inner_per_crate"],
        "top_n_workspace": sig["top_n_workspace"],
    }


def _is_src_file(file_path: str) -> bool:
    """True if the file lives under src/ rather than tests/ / benches/ / examples/.
    Architectural seams live in src/; test-harness seams crowd the seam list otherwise --
    e.g. sourcetrait_empower's process_spawn signal was dominated by tests/*.rs Host
    harnesses before this filter landed. Handles two layouts: per-crate tests under
    `crates/<X>/tests/...` (slash-segment form) and workspace-top-level tests under
    `tests/...` (path-prefix form, as nushell uses for integration tests). Used by
    detected_seams to keep S3 focused on the architectural signal."""
    excluded = ("tests/", "benches/", "examples/")
    if any(file_path.startswith(p) for p in excluded):
        return False
    return all(f"/{p}" not in file_path for p in excluded)


def _macro_defs_index(facts: dict):
    """Build a {macro_name: sorted [(file, line)] list} dict from facts['macro_defs'],
    deduped by (name, file, line) so per-crate roll-ups (same definition counted under
    both the per-crate name and the workspace 'nu' pseudo-crate) collapse cleanly.

    0.0.4 patch 3 (t) helper: catches `macro_rules! NAME { ... }` blocks captured by
    rustscan so emit_orientation can annotate the registration UNRESOLVED with the
    inline-definition span(s) instead of blanket-asserting 'expansion invisible'."""
    idx = defaultdict(list)
    seen = set()
    for m in facts.get("macro_defs", []):
        name = m.get("name")
        file_ = m.get("file", "?")
        line = m.get("line", "?")
        if not name:
            continue
        key = (name, file_, line)
        if key in seen:
            continue
        seen.add(key)
        idx[name].append((file_, line))
    return {name: sorted(spans) for name, spans in idx.items()}


def detected_seams(fp: dict, facts: dict):
    """Seams the static trace cannot cross - seeds for the UNRESOLVED guardrail list.
    Spawn-site filtering is src/-only so test-harness sites do not crowd the architectural
    signal (see _is_src_file).

    0.0.4 patch 3 (t): the macro-mediated-registration seam description now reports how
    many of the listed macros have inline `macro_rules!` definitions in the workspace,
    pointing the reader at S6 for per-macro detail with definition spans."""
    seeds = []
    inv = fp.get("seam_inventory", {})
    if inv.get("process_spawn") or inv.get("std_io_stream"):
        spawn_sites = [u for u in facts["uses"]
                       if "process" in u.get("path", "") and _is_src_file(u.get("file", ""))]
        seeds.append(("process / IPC boundary",
                      "A child process or stdin/stdout protocol crosses an address-space "
                      "boundary; static tracing stops here. Verify the wire format in source "
                      "before authoring across it.",
                      spawn_sites[:5]))
    if fp.get("registration_macros"):
        macs = ", ".join(fp["registration_macros"].keys())
        macro_defs_idx = _macro_defs_index(facts)
        inline_count = sum(1 for m in fp["registration_macros"] if m in macro_defs_idx)
        total = len(fp["registration_macros"])
        desc = ("Items are registered by a macro; the call-site argument list is "
                "captured but the EXPANSION is not visible to the floor scanner. Counts "
                "are unverified without the rustdoc overlay.")
        if inline_count > 0:
            desc += (f" {inline_count} of {total} macros have inline `macro_rules!` "
                     f"definitions in the workspace - see S6 for per-macro detail with "
                     f"definition spans (expansion is statically followable for those).")
        else:
            desc += (" Confirm the generated items in source or via overlay before "
                     "relying on the registry.")
        seeds.append((f"macro-mediated registration ({macs})", desc, []))
    if inv.get("extern") or inv.get("syscall_libc"):
        seeds.append(("FFI / syscall boundary",
                      "`extern`/`libc` crosses into non-Rust or the kernel; static tracing "
                      "stops at the boundary. Verify the foreign contract before authoring.",
                      []))
    return seeds


def emit_container_routing(root: Path, fp: dict, facts: dict, out: Path):
    """0.0.8 patch 8e: routing-doc orientation shape for workspaces
    classified as containers by the shape heuristic.

    When classify_workspace_shape returns shape=container (sourcetrait_
    common as the canonical case), the workspace has no single
    architectural pattern - each member is a separate topical library
    and the picker's output would be confidently wrong. This emitter
    produces a routing-doc shape that points the agent at the per-
    member sub-orientations instead.

    The provenance + per-crate listing + histogram appendix are still
    rendered (they remain auditable) but the worked-slice / authoring-
    guide sections are replaced by a routing prompt."""
    shape = fp.get("workspace_shape", {})
    signals = shape.get("signals", {})
    L = ["# Orientation: Container Workspace", "",
         "The structural signals indicate this workspace is a container - "
         "a sub-topic aggregator with no single architectural pattern. "
         "Each member is a separate topical library; running the picker "
         "across the workspace as a whole would produce confidently-wrong "
         "output. This artifact routes you to the per-member orientations "
         "instead.", "",
         "```", provenance(root, out.parent, fp), "```", ""]

    L += ["## How this artifact was shaped", "",
          "- shape: **container** (picker bypassed)",
          f"- reasoning: {shape.get('reasoning', '?')}",
          f"- central_crate: `{signals.get('central_crate', '?')}` "
          f"(dominant kind: {signals.get('central_kind', '?')})",
          f"- uniqueness_ratio: {signals.get('uniqueness_ratio', '?')} "
          f"(higher -> patterns more crate-isolated)",
          f"- kind_dominance_dispersion: "
          f"{signals.get('kind_dominance_dispersion', '?')} (higher -> "
          f"crates have diverse dominant kinds)",
          f"- leaf_ratio: {signals.get('leaf_ratio', '?')}",
          f"- hub_centrality: {signals.get('hub_centrality', '?')}",
          ""]

    # Workspace members listing
    L += ["## Workspace members", "",
          "Each member is a separate topical library. To produce an "
          "architectural orientation for a specific topic, run know_rust "
          "against the sub-workspace of interest.", ""]
    for name in sorted(fp["per_crate"]):
        c = fp["per_crate"][name]
        ideps = [d for d in c["deps"] if d in fp["per_crate"]]
        dep_str = f" -> depends on: {', '.join(ideps)}" if ideps else ""
        L.append(f"- **{name}** ({c['dir']}/, {c['sloc']} SLOC, "
                 f"{c['n_impls']} impls, {c['n_types']} types){dep_str}")
    L.append("")
    L.append("**[AGENT]** Pick the member whose architectural pattern "
             "you want to trace, then run know_rust against that "
             "member's directory. Open the corresponding orientation.md "
             "for the per-topic worked slices and authoring guides. The "
             "container workspace itself has no unifying architectural "
             "pattern to trace; do not author across member boundaries "
             "without first reading each member's individual orientation.")
    L.append("")

    # Histogram appendix (for auditing)
    L += ["## Appendix: full pattern histogram", "",
          "Reported for auditing the container annotation. If the "
          "histogram surfaces a single dominant architectural pattern "
          "that you believe IS the workspace's central pattern, consider "
          "removing the `container = true` annotation. Otherwise the "
          "flat distribution typical of containers should be visible "
          "here.", ""]
    for row in fp["pattern_histogram"][:25]:
        L.append(f"- `{row['pattern']}` - {row['count']}")
    L.append("")

    out.write_text("\n".join(L))


# 0.0.16 patch 16b: per-classification modifier for the Tier 1/2/3
# coverage tiering prose. Surfaces the workspace's consumership-axis
# implications inline with the existing coverage guidance. dev_use
# and end_with_dev_use have empirical anchors in the 10-target
# reference set; dev_with_end_use and end_use are speculative until
# representative targets (gitoxide, etc.) are added.
_USE_TIER_MODIFIERS = {
    "dev_use": (
        "Workspace is a **dev_use library** consumed by other "
        "developers. The ARCHITECTURE set (5.1) is the workspace's "
        "doubly-strong external API surface; the PUBLIC set (5.2) "
        "is the broader public-by-example face; the INTER-CRATE "
        "set (5.3) is the library's internal composition flow; "
        "the CLIQUE set (5.4) is shared infrastructure broadly "
        "supported across the library's crates via STV vote. "
        "INNER-CRATE (5.6) per-crate shows where each library "
        "crate's own architecture lives."
    ),
    "end_with_dev_use": (
        "Workspace ships an **end-user product** "
        "(end_with_dev_use) with internal libraries composing it. "
        "The INTER-CRATE set (5.3) captures cross-crate flow that "
        "makes the product work; the PUBLIC set (5.2) is the "
        "(often narrower) external face the product offers to "
        "embedders or extension authors; the CLIQUE set (5.4) "
        "is the product's shared infrastructure by broad-consensus "
        "election. INTRA-CRATE (5.5) within the product's primary "
        "crate shows what it consumes from the internal libraries "
        "(minus clique); INNER-CRATE (5.6) shows each library "
        "crate's own architecture."
    ),
    "dev_with_end_use": (
        "Workspace's primary deliverable is a **library with an "
        "auxiliary CLI** (dev_with_end_use, gitoxide pattern). "
        "The PUBLIC set (5.2) is the library API; the INTER-CRATE "
        "set (5.3) is the cross-crate flow within the lib; the "
        "CLIQUE set (5.4) is shared infrastructure across the "
        "lib's crates by broad-consensus election. The CLI binary "
        "is part of the public face but should be treated as a "
        "thin wrapper around the lib unless its own complexity "
        "warrants attention. Note: this bucket has no empirical "
        "anchor in the current 10-target reference set; guidance "
        "is speculative until a gitoxide-class target probes "
        "through (see notes/know_rust/next-phase-agent-"
        "augmentation.md for the related 5th-bucket arbitration "
        "hatch debt)."
    ),
    "end_use": (
        "Workspace is a pure **end-user product** (end_use). "
        "Architecture / public / inter-crate / clique sets are "
        "the user-facing entry points and the cross-crate flows "
        "that compose the product's behavior. No external library "
        "face to track. Note: this bucket has no empirical anchor "
        "in the current 10-target reference set; guidance is "
        "speculative."
    ),
}


def emit_orientation(root: Path, fp: dict, facts: dict, out: Path):
    sel = fp["selection"]
    core, core_types, core_traits = core_vocabulary(fp, facts)
    cands = candidate_instances(fp, facts)
    # 0.0.7 patch 7c: emit S5 + S7 iterate the candidate_instances list to render
    # multi-protagonist worked slices and per-protagonist authoring guides.
    seams = detected_seams(fp, facts)

    L = ["# Orientation", "",
         "Read-first. This is the map; the source is the territory and `reference.md` is the",
         "exhaustive index. Map-first ordering: skeleton (crate map, core vocabulary, seams,",
         "flow) then the worked slice (the authoring template), then guardrails, then the",
         "authoring guide. Every claim is a span you can open. Sections marked **[AGENT]** are",
         "filled by reading source at the cited spans - never from guesswork.", "",
         "```", provenance(root, out.parent, fp), "```", ""]

    # method-selection honesty
    L += ["## How this artifact was shaped", "",
          f"- mode: **{sel['mode']}** (histogram alone: {sel['histogram_mode']}, "
          f"top_share={sel['top_share']})"]
    # 0.0.8 patch 8e: surface the structural shape label so the agent
    # knows what kind of workspace they're looking at without having to
    # re-derive it from the signals.
    shape_info = fp.get("workspace_shape", {})
    shape_label = shape_info.get("shape")
    if shape_label and shape_label != "container":
        L.append(f"- structural shape: **{shape_label}** -- "
                 f"{shape_info.get('reasoning', '')}")
    # 0.0.16 patch 16a: surface the workspace use-classification so
    # the agent knows the workspace's consumership axis (dev_use
    # libraries consumed by other developers, end_with_dev_use
    # products that ship to end-users with libs composing them,
    # dev_with_end_use libraries with CLI auxiliaries, end_use pure
    # binaries). Mechanically derived from per-crate has_bin /
    # has_lib + cross-crate is_pub usage at characterize.py
    # _classify_workspace_use (0.0.15 patch 15a).
    use_info = fp.get("workspace_use_classification", {})
    use_label = use_info.get("workspace") if isinstance(use_info, dict) else None
    if use_label:
        L.append(f"- use classification: **{use_label}** -- "
                 f"{use_info.get('reasoning', '')}")
    if sel.get("runner_up"):
        L.append(f"- **UNRESOLVED (method-selection):** runner-up mode `{sel['runner_up']}` "
                 f"is within the ambiguity band. Confirm against the histogram below.")
    for n in sel.get("notes", []):
        L.append(f"- note: {n}")
    L.append("")

    # 1. crate / region map
    L += ["## 1. Crate / region map", ""]
    # 0.0.4 patch 4 (u): pre-cluster crates by name prefix when the workspace has more
    # than _CLUSTER_THRESHOLD crates and at least one prefix group reaches the minimum
    # cluster size. The per-crate detail still follows for grep completeness.
    use_clusters = len(fp["per_crate"]) > _CLUSTER_THRESHOLD
    clusters, others = ({}, [])
    if use_clusters:
        clusters, others = _cluster_crates_by_prefix(fp["per_crate"].keys())
        if not clusters:
            use_clusters = False
    if fp["n_components"] > 1 or len(fp["workspace_roots"]) > 1:
        L.append(f"**Regional** - {fp['n_components']} disjoint component(s), "
                 f"{len(fp['workspace_roots'])} workspace root(s). Each component is a region; "
                 f"the seam-spine (S3) is the join. **[AGENT]** name each region's role and "
                 f"the named seams connecting it to the others; if two regions share no traced "
                 f"data path, record that as an UNRESOLVED rather than inventing a link.")
        L.append("")
        for idx, comp in enumerate(fp["components"], 1):
            L.append(f"- region {idx}: {', '.join(comp)}")
    else:
        L.append("Single connected component. Crates and their internal dependencies:")
    L.append("")
    if use_clusters:
        L.append("### 1.1 Crate clusters (by name prefix)")
        L.append("")
        L.append(f"Workspace has {len(fp['per_crate'])} crates; the prefix-grouping below "
                 f"surfaces architectural clusters above the per-crate detail. Threshold for "
                 f"clustering: {_CLUSTER_THRESHOLD} crates (env: ORIENT_CLUSTER_THRESHOLD). "
                 f"Minimum cluster size: {_CLUSTER_MIN_SIZE} (env: ORIENT_CLUSTER_MIN_SIZE). "
                 f"Snake-case (`name_x`) and kebab-case (`name-x`) prefixes are kept "
                 f"distinct.")
        L.append("")
        for prefix in sorted(clusters):
            members = sorted(clusters[prefix])
            sep = '_' if '_' in prefix else '-'
            L.append(f"- **`{prefix}{sep}*`** ({len(members)} crates): "
                     f"{', '.join(members)}")
        if others:
            L.append(f"- **Other** ({len(others)} crates): {', '.join(others)}")
        L.append("")
        L.append("### 1.2 Per-crate detail")
        L.append("")
    for name in sorted(fp["per_crate"]):
        c = fp["per_crate"][name]
        ideps = [d for d in c["deps"] if d in fp["per_crate"]]
        dep_str = f" -> depends on: {', '.join(ideps)}" if ideps else ""
        L.append(f"- **{name}** ({c['dir']}/, {c['sloc']} SLOC, {c['n_impls']} impls, "
                 f"{c['n_types']} types){dep_str}")
    L.append("")
    if use_clusters:
        L.append("**[AGENT]** In 2-4 sentences each, describe each crate cluster (S1.1) and "
                 "the load-bearing standalone crates from S1.2 / 'Other'. Populate *why* only "
                 "from crate-level doc-comments / README; where absent, write "
                 "`why: unverified`.")
    else:
        L.append("**[AGENT]** In 2-4 sentences each (what / why / where), describe the role "
                 "of the core crates. Populate *why* only from crate-level doc-comments / "
                 "README; where absent, write `why: unverified`.")
    L.append("")

    # 2. core type vocabulary
    L += ["## 2. Core type vocabulary", "",
          f"Most-depended-on crate: **{core}** - its public types are the vocabulary other "
          f"crates speak in. Items below are ranked by impl-block usage (descending) with "
          f"alphabetical-by-name as the tiebreaker (0.0.4 patch 2; was alphabetical at "
          f"0.0.3). Confirm and describe each (what / where load-bearing; why from "
          f"doc-comments else unverified):", ""]
    for t in core_traits[:40]:
        usage = t.get("_usage", 0)
        usage_str = (f"  *({usage} impl{'s' if usage != 1 else ''})*"
                     if usage > 0 else "")
        doc = (f" - doc: {t['doc'][:120]}" if t.get("doc")
               else "  *(why: unverified - no doc)*")
        L.append(f"- trait `{t['name']}` - {sp(t)}{usage_str}{doc}")
    for t in core_types[:40]:
        usage = t.get("_usage", 0)
        # Type usage is impl-target count + use-statement-occurrence count, mixed; label
        # it as "usage" to avoid claiming all are impl-targets.
        usage_str = (f"  *({usage} usage{'s' if usage != 1 else ''})*"
                     if usage > 0 else "")
        doc = (f" - doc: {t['doc'][:120]}" if t.get("doc")
               else "  *(why: unverified - no doc)*")
        L.append(f"- `{t['kind']} {t['name']}` - {sp(t)}{usage_str}{doc}")
    L.append("")

    # 3. seam-spine
    L += ["## 3. Seam-spine", "",
          "Where the workspace stops being one connected thing. These are where architect-",
          "level features add or cross boundaries, and where the static trace stops honestly.",
          ""]
    if seams:
        for title, desc, sites in seams:
            L.append(f"- **{title}** - {desc}")
            for s in sites:
                L.append(f"    - site: {s.get('path','')} ({s.get('file','?')}:{s.get('line','?')})")
    else:
        L.append("- No strong seam markers detected. **[AGENT]** confirm by inspecting the "
                 "dominant pattern's boundaries; absence of markers is itself worth noting.")
    L.append("")
    L.append("**[AGENT]** For each seam, trace each protagonist pattern's "
             "instance (per S5.N) UP TO the seam and stop. Record the wire / "
             "foreign contract location if visible; otherwise write `UNRESOLVED: "
             "<what you looked for>, <what you ran>`. Different protagonists may "
             "interact with the same seam differently (e.g. plugin commands cross "
             "the IPC seam; builtin commands do not).")
    L.append("")

    # 4. data-flow narrative (agent)
    L += ["## 4. Data-flow narrative", "",
          "**[AGENT]** Trace how the core data type (from S2) moves from entry to result "
          "through the core crates. 1-2 short paragraphs, each sentence anchored to a span "
          "from reference.md. Stop at any seam from S3 with an explicit UNRESOLVED.", ""]

    # 0.0.22 + 0.0.24 + 2026-06-05 clique: cands is a structured dict
    # with six significance sets: architecture / public / inter-crate
    # / clique (workspace-wide) + intra-crate / inner-crate (per-crate,
    # strict origin split). Render order is by importance per the_user
    # 2026-06-04. Clique replaces the prior 'internals' (the_user
    # 2026-06-05): STV election with each crate's intra top-N as a
    # ranked ballot, K = workspace top-N seats, Droop quota.
    intra_crate_per_crate = cands.get("intra_crate_per_crate", {}) if isinstance(cands, dict) else {}
    inner_per_crate = cands.get("inner_crate_per_crate", {}) if isinstance(cands, dict) else {}
    inter_crate_sig = cands.get("inter_crate", {}) if isinstance(cands, dict) else {}
    public_sig = cands.get("public", {}) if isinstance(cands, dict) else {}
    architecture_sig = cands.get("architecture", {}) if isinstance(cands, dict) else {}
    clique_sig = cands.get("clique", {}) if isinstance(cands, dict) else {}
    top_n_intra_crate_per_crate = (cands.get("top_n_intra_crate_per_crate", {})
                             if isinstance(cands, dict) else {})
    top_n_inner_per_crate = (cands.get("top_n_inner_per_crate", {})
                             if isinstance(cands, dict) else {})
    top_n_workspace = (cands.get("top_n_workspace", _TOP_N_FLOOR)
                      if isinstance(cands, dict) else _TOP_N_FLOOR)
    all_per_crate_top_ns = (list(top_n_intra_crate_per_crate.values())
                            + list(top_n_inner_per_crate.values()))
    per_crate_min = min(all_per_crate_top_ns) if all_per_crate_top_ns else _TOP_N_FLOOR
    per_crate_max = max(all_per_crate_top_ns) if all_per_crate_top_ns else _TOP_N_FLOOR

    # 5. significance sets
    L += ["## 5. Significance sets - the authoring templates", ""]
    if per_crate_min == per_crate_max == top_n_workspace:
        L.append(f"Top {top_n_workspace} per set. Each section below "
                 f"preserves its axis.")
    elif per_crate_min == per_crate_max:
        L.append(f"Top {top_n_workspace} for workspace-wide sets "
                 f"(architecture / public / inter-crate); top "
                 f"{per_crate_min} per crate for intra-crate / "
                 f"inner-crate. SLOC-scaled per "
                 f"`max(7, round(7 + 2 * log2(SLOC / DIVISOR)))`. "
                 f"the_user 2026-06-04: 'i'd rather slightly "
                 f"over-produce than under produce'.")
    else:
        L.append(f"Top {top_n_workspace} for workspace-wide sets "
                 f"(architecture / public / inter-crate); per-crate "
                 f"top-N for intra-crate / inner-crate ranges "
                 f"{per_crate_min}..{per_crate_max}, scaled per each "
                 f"crate's SLOC via `max(7, round(7 + 2 * log2(SLOC "
                 f"/ DIVISOR)))`. the_user 2026-06-04: 'i'd rather "
                 f"slightly over-produce than under produce'.")
    L.append("")
    if (architecture_sig or public_sig or inter_crate_sig
            or clique_sig or intra_crate_per_crate or inner_per_crate):
        # 5.1 Architecture (cross-crate AND public).
        L.append(f"### 5.1 Architecture significance "
                 f"({len(architecture_sig)} significant; "
                 f"workspace-wide cross-crate AND public-by-example)")
        L.append("")
        for pattern in sorted(architecture_sig.keys(),
                              key=lambda p: -architecture_sig[p]["count"]):
            e = architecture_sig[pattern]
            L.append(f"- `{pattern}` - architecture score "
                     f"{e['count']:.2f} - seed {sp(e['instance'])}")
        L.append("")
        # 5.2 Public (public-by-example only).
        L.append(f"### 5.2 Public significance "
                 f"({len(public_sig)} significant; "
                 f"workspace-wide public-by-example only)")
        L.append("")
        for pattern in sorted(public_sig.keys(),
                              key=lambda p: -public_sig[p]["count"]):
            e = public_sig[pattern]
            L.append(f"- `{pattern}` - public score "
                     f"{e['count']:.2f} - seed {sp(e['instance'])}")
        L.append("")
        # 5.3 Inter-crate (cross-crate flow only).
        L.append(f"### 5.3 Inter-crate significance "
                 f"({len(inter_crate_sig)} significant; "
                 f"workspace-wide cross-crate flow only)")
        L.append("")
        for pattern in sorted(inter_crate_sig.keys(),
                              key=lambda p: -inter_crate_sig[p]["count"]):
            e = inter_crate_sig[pattern]
            L.append(f"- `{pattern}` - inter_count {e['count']} - "
                     f"seed {sp(e['instance'])}")
        L.append("")
        # 5.4 Clique (workspace-wide STV election over per-crate intra
        # ballots). 2026-06-05 replacement for the prior 'internals'
        # sum-of-counts set: each crate has equal voting power so
        # the result captures broad cross-crate consensus rather than
        # one-crate domination. Droop quota; ballot = each crate's
        # intra top-N ordered by count.
        L.append(f"### 5.4 Clique significance "
                 f"({len(clique_sig)} elected; "
                 f"workspace-wide STV over per-crate intra ballots)")
        L.append("")
        for pattern in sorted(clique_sig.keys(),
                              key=lambda p: -clique_sig[p]["count"]):
            e = clique_sig[pattern]
            L.append(f"- `{pattern}` - clique votes {e['count']:.2f} - "
                     f"seed {sp(e['instance'])}")
        L.append("")
        # 5.5 Intra-crate (per crate; this crate's usage of OTHER
        # workspace crates' patterns, after dedup vs clique).
        L.append("### 5.5 Intra-crate significance (per crate; "
                 "patterns this crate uses with origin in OTHER "
                 "workspace crates, after dedup vs clique)")
        L.append("")
        for crate in sorted(intra_crate_per_crate.keys()):
            entries = intra_crate_per_crate[crate]
            if not entries:
                continue
            L.append(f"#### crate `{crate}` ({len(entries)} significant)")
            L.append("")
            for pattern in sorted(entries.keys(),
                                  key=lambda p: -entries[p]["count"]):
                e = entries[pattern]
                L.append(f"- `{pattern}` - {e['count']} occurrences "
                         f"- seed {sp(e['instance'])}")
                L.append("")
        # 5.6 Inner-crate (per crate; this crate's own architecture -
        # origin AND usage in the crate).
        L.append("### 5.6 Inner-crate significance (per crate; "
                 "patterns originating IN and used IN this crate - "
                 "the crate's own architecture)")
        L.append("")
        for crate in sorted(inner_per_crate.keys()):
            entries = inner_per_crate[crate]
            if not entries:
                continue
            L.append(f"#### crate `{crate}` ({len(entries)} significant)")
            L.append("")
            for pattern in sorted(entries.keys(),
                                  key=lambda p: -entries[p]["count"]):
                e = entries[pattern]
                L.append(f"- `{pattern}` - {e['count']} occurrences "
                         f"- seed {sp(e['instance'])}")
                L.append("")
        L.append("**[AGENT]** Coverage tiering per the_user "
                 "2026-06-03 / 2026-06-04 directives (sets ordered "
                 "by importance):")
        L.append("")
        L.append("- **Tier 1 (heaviest coverage)**: the ARCHITECTURE "
                 "set (5.1, cross-crate AND public-by-example). "
                 "Doubly-strong architectural protagonists - they "
                 "flow across the workspace AND surface in its "
                 "examples; allocate the deepest worked-slice "
                 "attention to each.")
        L.append("- **Tier 2 (heavy coverage)**: the PUBLIC set "
                 "(5.2, public-by-example only) and the INTER-"
                 "CRATE set (5.3, cross-crate flow only). Single-"
                 "signal workspace-wide patterns - still load-"
                 "bearing.")
        L.append("- **Tier 3 (workspace-internal shared)**: the "
                 "CLIQUE set (5.4, STV election over per-crate intra "
                 "ballots). Patterns elected by broad cross-crate "
                 "consensus when each crate gets equal voting power "
                 "- shared infrastructure the inter-crate top-N "
                 "didn't surface.")
        L.append("- **Tier 4 (per-crate coverage)**: INTRA-CRATE "
                 "(5.5) and INNER-CRATE (5.6) per-crate sets. "
                 "Intra-crate shows what each crate USES from "
                 "elsewhere (after dedup vs clique); inner-"
                 "crate shows each crate's own architecture "
                 "(defined here + used here). Mention with context "
                 "for the crate's role.")
        L.append("- **Tier 5 (baseline coverage)**: patterns NOT "
                 "in any set's top picks. The reference index "
                 "(`reference.md`) is the inventory; no per-"
                 "pattern attention beyond the listing.")
        L.append("")
        L.append("A pattern appearing in multiple sets is a "
                 "stronger signal - the `categories` field on each "
                 "pick surfaces multi-set membership.")
        L.append("")
        # 0.0.16 patch 16b: classification-aware tier modifier. Per-
        # bucket guidance from _USE_TIER_MODIFIERS surfaces what the
        # workspace's consumership axis means for coverage decisions.
        if use_label and use_label in _USE_TIER_MODIFIERS:
            L.append(_USE_TIER_MODIFIERS[use_label])
            L.append("")
        # 0.0.17 patch 17a: framework adoption - surface what-why-where-
        # axes writing-process principles in the [AGENT] prompts. The
        # picker output is the structured starting material; the framework
        # tells the agent how to THINK about each picked pattern before
        # composing prose. See notes/know_rust/what-why-where-axes.md
        # for the rolling design (axis quick view at the top + per-
        # category decomposition + pass discipline + reduction-through-
        # inference sections).
        L.append("**[AGENT] Authoring guidance.** The picker hands you "
                 "patterns; the framework tells you HOW to write about "
                 "them. Four cues:")
        L.append("")
        L.append("- **Form vs role.** Each pattern's `kind:name` is its "
                 "FORM (mechanical, derived from syntax: trait_impl / "
                 "derive / type_usage / reg_macro). Its ROLE is "
                 "semantic and surfaces from signals - is_pub + "
                 "inter_ratio + curated_example_count + the workspace's "
                 "use classification (above). Most items align (form = "
                 "role); when they diverge (a fn whose role is data-"
                 "modeling like `to_string`; a struct whose role is "
                 "functional like a builder), surface BOTH explicitly.")
        L.append("- **Per-category decomposition.** Pick the category "
                 "first then think through its data slots: "
                 "*functional* (operation + parameterized input + "
                 "state read + parameterized output + state mutated); "
                 "*data-modeling* (broad category + sub-categories + "
                 "sub-representational ops + transformative ops + "
                 "intended use); *labeling* (load-bearing vs "
                 "considered-but-arbitrary vs broadly insignificant); "
                 "*organizing* (means of containment + items "
                 "maintained + parent context + structural shape).")
        L.append("- **Pass discipline.** Obvious pass writes from "
                 "source + doc-comments at the cited spans. Return "
                 "pass re-reads for skimmed slots and marks "
                 "UNRESOLVED rather than backfilling with speculation. "
                 "UNRESOLVED is a guardrail applied PER ITEM, not "
                 "only per seam (S6).")
        L.append("- **Reduction through inference.** The reader sees "
                 "`kind:name` + the span - don't restate what name + "
                 "form already convey. Spend the prose budget on the "
                 "non-inferrable residual: gotchas, edge cases, "
                 "internal-vs-external state effects, call-site "
                 "context, workspace invariants. A summary that says "
                 "'Parses an input string into a Command' tells the "
                 "reader nothing they didn't already infer; one that "
                 "says 'Strict parser; rejects empty strings; does "
                 "NOT handle quoting (upstream tokenizer); shared by "
                 "batch + REPL invocations' is residual.")
        L.append("")
    # End of section 5: the six-set significance sections above are
    # the complete picker output. No further per-protagonist rendering.
    L.append("")

    # 6. UNRESOLVED guardrails
    L += ["## 6. UNRESOLVED guardrails", "",
          "Do not author *across* these without verifying in source first - a guessed "
          "bridge compiles but is wrong. Seeded from detected boundaries; **[AGENT]** add "
          "any trace stop you hit.", ""]
    if fp.get("registration_macros"):
        # 0.0.4 patch 3 (t): per-macro inline-definition lookup so the per-macro entry
        # downgrades from "expansion invisible" to "expansion readable inline at <span>"
        # when an inline `macro_rules!` definition is captured in facts['macro_defs'].
        macro_defs_idx = _macro_defs_index(facts)
        for mac, n in fp["registration_macros"].items():
            defs = macro_defs_idx.get(mac, [])
            if defs:
                shown = defs[:5]
                tail = ("" if len(defs) <= 5
                        else f" (+ {len(defs) - 5} more definition site(s))")
                spans_str = ", ".join(f"{f}:{ln}" for f, ln in shown)
                L.append(f"- **`{mac}!` registration** - expansion READABLE inline in "
                         f"the workspace at {spans_str}{tail}. Call-site arg counts "
                         f"remain unverified without the rustdoc overlay, but the "
                         f"`macro_rules!` body is statically followable for each listed "
                         f"definition site. Different definition crates may expand "
                         f"differently; confirm per call-site crate.")
            else:
                L.append(f"- **`{mac}!` registration** - expansion not visible to the "
                         f"floor scanner (no inline `macro_rules!` definition found in "
                         f"the workspace; may be a proc-macro, imported from an external "
                         f"crate, or otherwise out of scope). Call-site arg counts are "
                         f"unverified. Confirm generated items in source or via the "
                         f"rustdoc overlay before relying on the registry.")
    for title, desc, _ in seams:
        L.append(f"- **{title}** - {desc}")
    if not fp.get("registration_macros") and not seams:
        L.append("- None seeded. Record trace stops here as you hit them.")
    L.append("")

    # 7. pattern-authoring guides
    L += ["## 7. Pattern-authoring guides", ""]
    # Tier 1-3 (architecture + public + inter-crate + clique) hold the
    # patterns most relevant to the workspace's consumership.
    cls_tag = (f"Workspace classification: **{use_label}**. "
               if use_label else "")
    L.append("**[AGENT]** From the trait / type definitions in "
             "S2 and the significance sets in S5, write the "
             "minimal checklist to author a NEW instance of one "
             "of the Tier 1-3 patterns (S5.1 architecture, "
             "S5.2 public, S5.3 inter-crate, or S5.4 clique). "
             + cls_tag +
             "Frame the checklist for the consumership the "
             "workspace serves: dev_use workspaces author against "
             "the library's public API; end_with_dev_use "
             "workspaces author internal-product features whose "
             "cross-crate flow matters; dev_with_end_use treats "
             "the lib as primary; end_use authors user-facing "
             "entry points. Anchor each step to a span. Apply the "
             "S5 authoring guidance per item: form-vs-role + "
             "per-category decomposition + pass discipline + "
             "reduction-through-inference.")
    L.append("")
    L += ["## Appendix: full pattern histogram", ""]
    for row in fp["pattern_histogram"][:25]:
        L.append(f"- `{row['pattern']}` - {row['count']}")
    L.append("")
    out.write_text("\n".join(L))


def main():
    if len(sys.argv) < 2:
        print("usage: emit.py <repo_root> [orientation_dir]", file=sys.stderr)
        return 2
    root = Path(sys.argv[1]).resolve()
    odir = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / ".orientation"
    fp = json.loads((odir / "fingerprint.json").read_text())
    facts = json.loads((odir / "facts.json").read_text())
    emit_reference(root, fp, facts, odir / "reference.md")
    # 0.0.8 patch 8e: workspace shape classifier dispatches container-
    # shape workspaces to the routing-doc orientation; all other shapes
    # use the standard worked-slice orientation. Heuristic-driven; no
    # annotation needed (the_user 2026-06-03: heuristics should analyze
    # correctly without source-code intervention).
    shape = fp.get("workspace_shape", {}).get("shape")
    if shape == "container":
        emit_container_routing(root, fp, facts, odir / "orientation.md")
    else:
        emit_orientation(root, fp, facts, odir / "orientation.md")
    orient_lines = (odir / "orientation.md").read_text().count("\n")
    ref_lines = (odir / "reference.md").read_text().count("\n")
    print(f"[emit] wrote orientation.md ({orient_lines} lines) and "
          f"reference.md ({ref_lines} lines)")
    print(f"[emit] orientation/reference line ratio = {orient_lines/max(1,ref_lines):.2f} "
          f"(orientation must stay well below source size)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())