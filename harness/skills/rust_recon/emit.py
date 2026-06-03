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

# 0.0.4 patch 2 (s): word-bounded capitalized identifier for use-path scanning.
_TYPE_IDENT_RE = re.compile(r'\b[A-Z]\w*\b')

# 0.0.5 patch dd: generic auto-derive trait names. These are derived universally via
# #[derive(...)] across the standard data-shaping needs (Debug for stringification,
# Clone / Copy for value semantics, the equality + hashing family for collections,
# Default for zero-value construction, Serialize / Deserialize for serde). They are
# NOT architecturally load-bearing - picking one as the worked-slice protagonist
# yields a generic trace that does not reveal the workspace's structural pattern.
# candidate_instance treats a generic-auto-derive leader as kind-only and walks past
# to find a domain-specific pattern. Domain-flavored derives that ARE load-bearing
# for their workspace (bevy's Component / Resource / System / Event, etc.) stay OUT
# of this set so they remain pickable.
_GENERIC_AUTO_DERIVES = frozenset([
    "Debug", "Clone", "PartialEq", "Eq", "Hash", "Default", "Copy",
    "Serialize", "Deserialize",
])

# 0.0.4 patch 4 (u): when the workspace has more than _CLUSTER_THRESHOLD crates, the
# S1 crate / region map emits a "Crate clusters (by name prefix)" sub-section above
# the per-crate detail list. Clusters require at least _CLUSTER_MIN_SIZE members.
_CLUSTER_THRESHOLD = int(os.environ.get("ORIENT_CLUSTER_THRESHOLD", "15"))
_CLUSTER_MIN_SIZE = int(os.environ.get("ORIENT_CLUSTER_MIN_SIZE", "3"))


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
    return (None, [])


def _is_generic_pattern(kind, name):
    """True when the pattern is a derive OR a manual trait_impl of one of the
    standard data-shaping traits (Debug / Clone / PartialEq / Eq / Hash / Default /
    Copy / Serialize / Deserialize).

    0.0.5 patch dd: derives of these traits are universally applied via
    #[derive(...)]; treating them as the worked-slice protagonist yields a trace
    that does not reveal the workspace's architectural pattern.

    0.0.6 patch 6a: extended to trait_impl variants of the same trait names. Manual
    impls of Debug / Clone / etc. are almost always data-shaping (custom formatting,
    optimized cloning) rather than architectural; tokio's 0.0.5 baseline picked
    trait_impl:Debug at 146 instances even though Debug formatting is not what
    tokio's architecture is about. Edge case: a debug-tracing library where manual
    Debug impls ARE architectural would mis-skip - watch for false positives during
    per-target audit.

    candidate_instance treats these as kind-only signals and walks past, the same
    way Patch 5 handles fn_table:<crate> leaders."""
    return name in _GENERIC_AUTO_DERIVES and kind in ("derive", "trait_impl")


def _is_workspace_defined(kind, name, workspace_traits, macro_defs_idx):
    """True when the pattern is anchored to a workspace-defined trait or macro.

    0.0.6 patch 6b: trait_impl:<T> and derive:<T> are workspace-defined when T
    appears in facts['traits'] (built into workspace_traits as a name set);
    reg_macro:<N> is workspace-defined when N appears in facts['macro_defs'] as
    an inline macro_rules! definition (Patch 3's tracking). The picker prefers
    workspace-defined patterns over imported ones; the reasoning is that the
    workspace's architectural pattern lives in traits and macros the workspace
    DEFINES, not in trait_impls of std::convert::From or external macros like
    fluent-localization's fl!.

    Helix's 0.0.5 baseline picked trait_impl:From (std-library) at rank 12 even
    though trait_impl:Request (helix-lsp-types::Request) at rank 16 is more
    architecturally specific; bevy's 0.0.5 baseline picked trait_impl:From over
    its many workspace-defined trait_impls (Plugin, Bundle, etc.) and derives
    (Component, Resource, etc.). This helper drives the two-pass walk that
    catches both cases.

    Detection caveat: name-based (no path resolution). False positives possible
    when a workspace defines a trait with the same name as a commonly-impl'd
    imported trait (e.g. `Request` in helix-lsp-types AND `tower::Request`).
    For 0.0.6 v1, name check is the simple defensible signal; refine with
    rustdoc-overlay path resolution in a later iteration if false positives
    surface."""
    if kind in ("trait_impl", "derive"):
        return name in workspace_traits
    if kind == "reg_macro":
        return name in macro_defs_idx
    return False


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
    histogram = fp["pattern_histogram"]
    workspace_traits = {t["name"] for t in facts.get("traits", []) if t.get("name")}
    macro_defs_idx = _macro_defs_index(facts)
    leader_dom = histogram[0]["pattern"]
    leader_kind, _, leader_name = leader_dom.partition(":")
    leader_inst, leader_spans = _instance_for_kind(leader_kind, leader_name, facts)
    leader_is_generic = _is_generic_pattern(leader_kind, leader_name)
    leader_is_workspace = _is_workspace_defined(
        leader_kind, leader_name, workspace_traits, macro_defs_idx)
    # Direct return only when the leader is valid + non-generic + workspace-defined.
    # 0.0.6 patch 6b: non-workspace leaders (trait_impl:From, reg_macro:fl, etc.)
    # fall through to the walk so a workspace-defined alternative gets a chance.
    if leader_inst is not None and not leader_is_generic and leader_is_workspace:
        return [{
            "kind": leader_kind,
            "pattern": leader_dom,
            "instance": leader_inst,
            "all_spans": leader_spans,
            "fallback_reason": None,
        }]
    # Two-pass walk. Priority: trait_impl > derive > reg_macro. Pass 1 restricts
    # to workspace-defined patterns; pass 2 (only runs if pass 1 finds nothing)
    # falls back to non-workspace patterns. Generic patterns are skipped in both
    # passes per Patch dd + 6a.
    priority_order = ("trait_impl", "derive", "reg_macro")

    def _walk(prefer_workspace):
        first_by_priority = {k: None for k in priority_order}
        for entry in histogram:
            dom = entry["pattern"]
            kind, _, name = dom.partition(":")
            if kind not in first_by_priority or first_by_priority[kind] is not None:
                continue
            if _is_generic_pattern(kind, name):
                continue
            if prefer_workspace and not _is_workspace_defined(
                    kind, name, workspace_traits, macro_defs_idx):
                continue
            inst, spans = _instance_for_kind(kind, name, facts)
            if inst is None:
                continue
            first_by_priority[kind] = (entry, inst, spans)
        for priority in priority_order:
            if first_by_priority[priority] is not None:
                return first_by_priority[priority]
        return None

    pick = _walk(prefer_workspace=True)
    pick_is_workspace = pick is not None
    if pick is None:
        pick = _walk(prefer_workspace=False)
    if pick is None:
        # No viable structural pattern at all; return leader with no instance.
        return [{
            "kind": leader_kind,
            "pattern": leader_dom,
            "instance": None,
            "all_spans": [],
            "fallback_reason": None,
        }]
    entry, inst, spans = pick
    dom = entry["pattern"]
    kind = dom.partition(":")[0]
    # Build fallback_reason explaining why the leader was passed over (if it was)
    # plus how the pick was chosen (workspace-defined vs fallback).
    if leader_is_generic:
        kind_descriptor = (
            "generic auto-derive" if leader_kind == "derive"
            else "manual impl of a generic data-shaping trait"
        )
        leader_reason = (
            f"is a {kind_descriptor} ({leader_name} is universally derived or "
            f"manually implemented across the standard data-shaping traits; "
            f"not architecturally load-bearing)"
        )
    elif leader_inst is None:
        leader_reason = (
            f"is a kind-only signal that does not yield a structurally "
            f"followable item to trace - it counts a category (free functions, "
            f"an external attribute macro, etc.) rather than a single concrete "
            f"pattern"
        )
    elif not leader_is_workspace:
        leader_reason = (
            f"is implemented for an imported trait (`{leader_name}` is not "
            f"declared in any workspace crate; the workspace's architectural "
            f"patterns live in workspace-defined traits / macros)"
        )
    else:
        # Shouldn't happen given the direct-return gate; defensive default.
        leader_reason = f"was passed over by the picker"
    workspace_note = (
        " The pick is a workspace-defined pattern (the trait or macro is "
        "declared inside a workspace crate)."
        if pick_is_workspace
        else " No workspace-defined alternative was available at any priority "
             "tier; the pick is the highest-rank non-workspace structural "
             "pattern."
    )
    return [{
        "kind": kind,
        "pattern": dom,
        "instance": inst,
        "all_spans": spans,
        "fallback_reason": (
            f"Histogram leader `{leader_dom}` ({histogram[0]['count']} instances) "
            f"{leader_reason}. Picked `{dom}` ({entry['count']} instances) as "
            f"the load-bearing alternative, prioritizing trait_impl > "
            f"non-generic-derive > reg_macro within the workspace-defined tier."
            f"{workspace_note}"
        ),
    }]


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


def emit_orientation(root: Path, fp: dict, facts: dict, out: Path):
    sel = fp["selection"]
    core, core_types, core_traits = core_vocabulary(fp, facts)
    cands = candidate_instances(fp, facts)
    # 0.0.7 patch 7a: candidate_instances returns a list; emit S5 still uses one
    # protagonist for now (cands[0] if any). 7c will iterate the full list to
    # produce the multi-protagonist S5 + S7 sections.
    cand = cands[0] if cands else None
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
        L.append(f"- **{name}** ({c['dir']}/, {c['loc']} LoC, {c['n_impls']} impls, "
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
    L.append("**[AGENT]** For each seam, trace the dominant pattern's instance UP TO the seam "
             "and stop. Record the wire/foreign contract location if visible; otherwise write "
             "`UNRESOLVED: <what you looked for>, <what you ran>`.")
    L.append("")

    # 4. data-flow narrative (agent)
    L += ["## 4. Data-flow narrative", "",
          "**[AGENT]** Trace how the core data type (from S2) moves from entry to result "
          "through the core crates. 1-2 short paragraphs, each sentence anchored to a span "
          "from reference.md. Stop at any seam from S3 with an explicit UNRESOLVED.", ""]

    # 5. worked slice (the protagonist; seeded)
    L += ["## 5. Worked slice - the authoring template", ""]
    if cand and cand.get("instance"):
        dom = cand["pattern"]
        inst = cand["instance"]
        fallback_reason = cand.get("fallback_reason")
        # Find this pattern's own count in the histogram (may not be position 0 when
        # the load-bearing fallback fired).
        own_count = next(
            (h["count"] for h in fp["pattern_histogram"] if h["pattern"] == dom), 0)
        if fallback_reason:
            L.append(f"Dominant pattern (load-bearing pick): **`{dom}`** "
                     f"({own_count} instances; this is the kind you will most often "
                     f"author).")
            L.append("")
            L.append(f"**Why this pattern:** {fallback_reason}")
        else:
            L.append(f"Dominant pattern: **`{dom}`** "
                     f"({own_count} instances; this is the kind you will most often "
                     f"author).")
        if cand["kind"] == "trait_impl":
            L.append(f"Seed instance: `impl {dom.split(':')[1]} for {inst.get('type')}` "
                     f"- {sp(inst)}.")
        else:
            L.append(f"Seed instance - {sp(inst)}.")
        L.append("")
        L.append("**[AGENT]** Trace THIS ONE instance across every crate boundary it touches, "
                 "as the executable template for authoring the next one:")
        L += ["- **what** it does: inputs / outputs / state + environment changes "
              "(load-bearing - read the impl body in source).",
              "- **where** it plugs in: how it is registered and invoked (follow the "
              "registration path; if it goes through a macro, that is a guardrail - see S6).",
              "- **why** it is shaped this way: from doc-comments only, else `why: unverified`.",
              "- stop honestly at each seam (S3) with `UNRESOLVED: what you looked for, what "
              "you ran`. A stop is a success - it marks a real boundary for the next author."]
        L.append("")
        L.append("Every other instance of this pattern (open any to compare): "
                 + ", ".join(f"`{s}`" for s in cand["all_spans"][:15])
                 + (" ..." if len(cand["all_spans"]) > 15 else ""))
    else:
        L.append("**[AGENT]** No single dominant instance was isolated automatically "
                 f"(mode: {sel['mode']}). Pick the largest pattern from the histogram below "
                 "and trace one instance of it as the template.")
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

    # 7. pattern-authoring guide
    L += ["## 7. Pattern-authoring guide", "",
          "**[AGENT]** From the trait/struct definitions in S2 and the worked slice in S5, "
          "write the minimal checklist to author a NEW instance of the dominant pattern: which "
          "trait to implement, which methods are required (read the trait def in source), how "
          "to register it (the path from S5), and which seams (S6) a new instance must "
          "respect. Anchor each step to a span.", "",
          "## Appendix: full pattern histogram", ""]
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
