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

# 0.0.8 patch 8c: generic-shape outer names for type_usage entries.
# A type_usage pattern `<outer>::<inner>` whose outer is in this set is
# treated as a generic trait-method invocation (Default::default,
# Display::fmt, From::from, Clone::clone, etc.) and skipped by the
# picker. These are noise in the architectural sense even though they
# pass the rustscan TYPE_USAGE_NOISE_TYPES gate (which only excludes
# noisy collection / smart-pointer outers). Workspace-defined outers
# are NEVER in this set; if a workspace declared a trait named one of
# these, the workspace_types lookup still surfaces the pattern.
_GENERIC_AUTO_TYPES = frozenset([
    # standard trait outers
    "Default", "Display", "Debug",
    "From", "Into", "TryFrom", "TryInto",
    "Clone",
    "AsRef", "AsMut",
    "Drop",
    "PartialEq", "Eq", "Hash", "PartialOrd", "Ord",
    "Iterator", "IntoIterator",
    # Poll / Future trait outers (used as enum constructors)
    "Poll",
])

# 0.0.8 patch 8c: minimum usage count for an external (non-workspace-
# defined) type_usage entry to count as a workspace-relevant lynchpin.
# Tokio's task::spawn (65) + Poll::Ready (450) pass; AtomicUsize::new
# (41 in tokio's own scan) doesn't. Configurable via env. Calibrated
# from the helix + tokio probes in 8b - workspace-defined types tend
# to land in 38-99 range; setting the lynchpin floor at 60 keeps
# noise (under-threshold std primitives) out while admitting the
# tokio::spawn class of external architectural patterns.
_LYNCHPIN_USAGE_MIN = int(os.environ.get("ORIENT_LYNCHPIN_USAGE_MIN", "60"))

# 0.0.9 patch 9a: inner method names treated as generic-shape for
# inner-grouping family detection. A type_usage with inner method
# `new` / `default` / `from` / `fmt` / etc. doesn't contribute to
# the inner-family signal because these names are universal across
# Rust types; grouping them by inner would produce a meaningless
# "all-constructors" family. Outer-grouping isn't affected - generic
# inner methods still aggregate into their outer's family.
_GENERIC_INNER_METHODS = frozenset([
    "new", "default", "from", "into", "try_from", "try_into",
    "fmt", "clone", "as_ref", "as_mut", "deref", "deref_mut",
    "eq", "ne", "cmp", "partial_cmp", "hash",
    "drop", "next", "iter", "into_iter", "iter_mut",
    "build", "into_inner", "borrow", "borrow_mut",
])

# 0.0.9 patch 9a: minimum distinct variants for a type-usage family.
# An outer-family requires >= this many distinct inner methods sharing
# the outer; an inner-family requires >= this many distinct outers
# sharing the inner. Selection::new + Selection::single + Selection::range
# qualifies the outer "Selection"; mpsc::channel + oneshot::channel
# qualifies the inner "channel". Single-variant outers (Selection only
# ever called as Selection::new) don't form a family.
_FAMILY_MIN_VARIANTS = int(os.environ.get("ORIENT_FAMILY_MIN_VARIANTS", "2"))

# 0.0.4 patch 4 (u): when the workspace has more than _CLUSTER_THRESHOLD crates, the
# S1 crate / region map emits a "Crate clusters (by name prefix)" sub-section above
# the per-crate detail list. Clusters require at least _CLUSTER_MIN_SIZE members.
_CLUSTER_THRESHOLD = int(os.environ.get("ORIENT_CLUSTER_THRESHOLD", "15"))
_CLUSTER_MIN_SIZE = int(os.environ.get("ORIENT_CLUSTER_MIN_SIZE", "3"))

# 0.0.7 patch 7b: candidate_instances surfaces up to this many architectural
# patterns per workspace via per-crate aggregation. Justified by the
# the_user-validated manual ground-truth list at
# notes/rust_recon/methodology_findings.md - most workspaces have 4-6
# architectural patterns; 5 is the central tendency. Configurable so future
# iterations can probe larger / smaller N.
_PLURAL_N = int(os.environ.get("ORIENT_PLURAL_N", "5"))


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
        inst = [tu for tu in facts.get("type_usages", [])
                if tu.get("name") == name]
        return (inst[0] if inst else None,
                [f"{tu.get('file','?')}:{tu['line']}" for tu in inst[:200]])
    if kind == "type_usage_family":
        # 0.0.9 patch 9a: family instances aggregate call sites from all
        # matching type_usages. outer:X matches names starting with X::;
        # inner:Y matches names ending with ::Y.
        if name.startswith("outer:"):
            outer = name.split(":", 1)[1]
            inst = [tu for tu in facts.get("type_usages", [])
                    if tu.get("name", "").split("::", 1)[0] == outer]
        elif name.startswith("inner:"):
            inner = name.split(":", 1)[1]
            inst = [tu for tu in facts.get("type_usages", [])
                    if tu.get("name", "").rpartition("::")[2] == inner]
        else:
            inst = []
        return (inst[0] if inst else None,
                [f"{tu.get('file','?')}:{tu['line']}" for tu in inst[:200]])
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

    0.0.8 patch 8c: extended to type_usage variants. A type_usage pattern
    `<outer>::<inner>` is generic when outer is in _GENERIC_AUTO_TYPES
    (Default::default, Display::fmt, From::from, Clone::clone, Poll::Ready,
    etc.) - these are standard trait-method invocations or enum-variant
    constructors that are universal across Rust code, not workspace-specific
    architectural patterns. Workspace-defined outers (Selection, Range, Rope
    for helix) survive because none of them are in the set.

    0.0.9 patch 9a: extended to type_usage_family. A family pattern name has
    the shape `outer:<X>` or `inner:<Y>`. Outer-families are generic when X
    is in _GENERIC_AUTO_TYPES (same as type_usage). Inner-families are NEVER
    generic at this stage because the inner-family construction (in
    _per_crate_top_patterns) already excludes _GENERIC_INNER_METHODS at
    counting time.

    candidate_instance treats these as kind-only signals and walks past, the same
    way Patch 5 handles fn_table:<crate> leaders."""
    if kind in ("derive", "trait_impl"):
        return name in _GENERIC_AUTO_DERIVES
    if kind == "type_usage":
        outer = name.split("::", 1)[0]
        return outer in _GENERIC_AUTO_TYPES
    if kind == "type_usage_family":
        if name.startswith("outer:"):
            outer = name.split(":", 1)[1]
            return outer in _GENERIC_AUTO_TYPES
        return False
    return False


def _is_workspace_defined(kind, name, workspace_traits, macro_defs_idx,
                          workspace_types=None, lynchpins=None):
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

    0.0.8 patch 8c: extended to type_usage. A type_usage pattern
    `<outer>::<inner>` is workspace-defined when its outer appears in
    workspace_types (the set of struct / enum / union / type aliases declared
    in the workspace). External lynchpins are also admitted: a non-workspace
    type_usage whose combined name appears in the lynchpins set (built from
    pattern_histogram entries above _LYNCHPIN_USAGE_MIN) counts as
    workspace-relevant. Tokio's task::spawn and Poll::Ready appear in the
    lynchpins set when scanning an application workspace using tokio; helix's
    Selection::new / Range::new appear via workspace_types when scanning helix
    itself.

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
    if kind == "type_usage":
        outer = name.split("::", 1)[0]
        if workspace_types and outer in workspace_types:
            return True
        if lynchpins and name in lynchpins:
            return True
        return False
    if kind == "type_usage_family":
        # 0.0.9 patch 9a: family patterns inherit workspace-defined
        # status from their variant outers. An outer-family `outer:X`
        # is workspace-defined when X is in workspace_types. An
        # inner-family `inner:Y` is workspace-defined when its family
        # name appears in the family lynchpins (passed alongside
        # single-entry lynchpins). For v1 the inner-family workspace
        # check is approximate; refinement candidate for 9a v2.
        if name.startswith("outer:"):
            outer = name.split(":", 1)[1]
            if workspace_types and outer in workspace_types:
                return True
        if lynchpins and name in lynchpins:
            return True
        return False
    return False


def _per_crate_top_patterns(facts, workspace_traits, macro_defs_idx,
                            workspace_types=None, lynchpins=None):
    """For each workspace crate, find the top non-generic workspace-defined
    pattern of each kind (trait_impl, derive, type_usage, reg_macro). Returns a
    dict of crate_name -> {kind -> {pattern, count, source_crate}}.

    0.0.7 patch 7b: this is the engine for plural protagonist surfacing. The
    global histogram counts patterns workspace-wide; per-crate aggregation
    discovers that each workspace crate has its own architectural lead (helix-
    core's Selection/Transaction, helix-view's Editor, helix-term's Command,
    helix-lsp's Request, etc.). Each crate's top non-generic workspace-defined
    pattern of each kind becomes a candidate protagonist; the aggregation
    phase combines + dedupes them into the workspace-wide plural list.

    0.0.8 patch 8c: type_usage is added as a 4th per-crate counter. Counts
    facts['type_usages'] entries whose outer name is workspace-defined (in
    workspace_types) or whose combined name is an external lynchpin (in
    lynchpins). Generic outers (Default::default, From::from, etc.) are
    skipped via _is_generic_pattern."""
    per_crate = defaultdict(
        lambda: {"trait_impl": Counter(), "derive": Counter(),
                 "reg_macro": Counter(), "type_usage": Counter(),
                 "type_usage_family": Counter()})
    for i in facts["impls"]:
        crate = i.get("crate")
        if not crate or i.get("cfg_gated"):
            continue
        name = i.get("trait")
        if not name:
            continue
        if _is_generic_pattern("trait_impl", name):
            continue
        if not _is_workspace_defined(
                "trait_impl", name, workspace_traits, macro_defs_idx,
                workspace_types, lynchpins):
            continue
        per_crate[crate]["trait_impl"][name] += 1
    for d in facts.get("derives", []):
        crate = d.get("crate")
        if not crate:
            continue
        name = d.get("trait")
        if not name:
            continue
        if _is_generic_pattern("derive", name):
            continue
        if not _is_workspace_defined(
                "derive", name, workspace_traits, macro_defs_idx,
                workspace_types, lynchpins):
            continue
        per_crate[crate]["derive"][name] += 1
    for m in facts.get("macros", []):
        if m.get("kind") != "macro_invocation":
            continue
        crate = m.get("crate")
        if not crate:
            continue
        name = m.get("name")
        if not name:
            continue
        if not _is_workspace_defined(
                "reg_macro", name, workspace_traits, macro_defs_idx,
                workspace_types, lynchpins):
            continue
        per_crate[crate]["reg_macro"][name] += 1
    for tu in facts.get("type_usages", []):
        crate = tu.get("crate")
        if not crate:
            continue
        name = tu.get("name")
        if not name:
            continue
        if _is_generic_pattern("type_usage", name):
            continue
        if not _is_workspace_defined(
                "type_usage", name, workspace_traits, macro_defs_idx,
                workspace_types, lynchpins):
            continue
        per_crate[crate]["type_usage"][name] += 1
    # 0.0.9 patch 9a: derive per-crate type_usage_family counters from
    # the per-crate type_usage Counter. For each crate, walk its
    # type_usage entries and build outer->inner-set + inner->outer-set
    # maps; a family qualifies when its variant count meets
    # _FAMILY_MIN_VARIANTS.
    for crate, kinds in per_crate.items():
        type_usage_counter = kinds["type_usage"]
        if not type_usage_counter:
            continue
        outer_inners = defaultdict(set)
        outer_count = Counter()
        inner_outers = defaultdict(set)
        inner_count = Counter()
        for name, count in type_usage_counter.items():
            if "::" not in name:
                continue
            outer, _, inner = name.partition("::")
            outer_inners[outer].add(inner)
            outer_count[outer] += count
            if inner and inner not in _GENERIC_INNER_METHODS:
                inner_outers[inner].add(outer)
                inner_count[inner] += count
        family_counter = Counter()
        for outer, count in outer_count.items():
            if len(outer_inners[outer]) >= _FAMILY_MIN_VARIANTS:
                family_counter[f"outer:{outer}"] = count
        for inner, count in inner_count.items():
            if len(inner_outers[inner]) >= _FAMILY_MIN_VARIANTS:
                family_counter[f"inner:{inner}"] = count
        kinds["type_usage_family"] = family_counter
    return per_crate


_PLURAL_KIND_PRIORITY = {"trait_impl": 0, "derive": 1,
                         "type_usage_family": 2, "type_usage": 3,
                         "reg_macro": 4}


def _aggregate_per_crate_picks(per_crate_counts, facts, n):
    """Flatten per-crate top picks into a single deduped list of candidates,
    capped at n, with kind-aware slot quotas. Returns a list of dicts in the
    candidate_instances return shape.

    0.0.7 patch 7b initial: pattern_to_pick deduped by pattern name, then
    sorted by raw count. This made reg_macro test helpers (nu!, current!,
    trace!) outrank architectural trait_impls on raw-count alone.

    0.0.7 patch 7b refinement A: ordering became (priority tier, raw count).
    Priority: trait_impl > derive > reg_macro. Surfaced architectural
    trait_impls but starved derive + reg_macro slots; bevy's derive:Component
    (architecturally central to ECS) never made the top 5 because trait_impl
    Plugin/MeshBuilder/etc. filled all slots.

    0.0.7 patch 7b refinement B: slot quotas. For N=5 the default split was
    3 trait_impl + 1 derive + 1 reg_macro. Quotas allocate by kind; underfilled
    kinds cede to the trait_impl pool so N total is maintained.

    0.0.8 patch 8c (this version): adds type_usage as a 4th kind with its
    own slot quota. At N=5 the default split becomes 2 trait_impl + 1 derive
    + 1 type_usage + 1 reg_macro. Output ordering by _PLURAL_KIND_PRIORITY:
    trait_impl > derive > type_usage > reg_macro, each tier sorted by count
    desc. Backfill on underfill draws from the trait_impl pool first."""
    pattern_to_pick = {}
    for crate, kinds in per_crate_counts.items():
        for kind in ("trait_impl", "derive", "type_usage",
                     "type_usage_family", "reg_macro"):
            counts = kinds.get(kind)
            if not counts:
                continue
            top_name, top_count = counts.most_common(1)[0]
            pattern = f"{kind}:{top_name}"
            existing = pattern_to_pick.get(pattern)
            if existing is None or top_count > existing["count"]:
                pattern_to_pick[pattern] = {
                    "kind": kind,
                    "pattern": pattern,
                    "count": top_count,
                    "source_crate": crate,
                }
    by_kind = {"trait_impl": [], "derive": [], "reg_macro": [],
               "type_usage": [], "type_usage_family": []}
    for entry in pattern_to_pick.values():
        by_kind[entry["kind"]].append(entry)
    for kind in by_kind:
        by_kind[kind].sort(key=lambda x: -x["count"])
    # 0.0.9 patch 9a: slot quotas at N=5 become 2 trait_impl + 1 derive
    # + 1 type_usage_family + 1 type_usage. Reg_macro drops to 0 because
    # the reg_macro slot in 0.0.7+ rarely surfaces architectural patterns
    # (helix's current!, ratatui's color!, tokio's trace! - none on the
    # manual ground-truth list). 9b's mode-adaptive quotas can restore
    # reg_macro for workspaces where reg_macro is the dominant kind.
    derive_quota = 1 if n >= 3 else 0
    type_usage_family_quota = 1 if n >= 4 else 0
    type_usage_quota = 1 if n >= 5 else 0
    reg_macro_quota = 0  # 9a default; 9b adapts
    trait_impl_quota = max(
        1,
        n - derive_quota - type_usage_family_quota - type_usage_quota
        - reg_macro_quota,
    )
    quotas = {
        "trait_impl": trait_impl_quota,
        "derive": derive_quota,
        "type_usage_family": type_usage_family_quota,
        "type_usage": type_usage_quota,
        "reg_macro": reg_macro_quota,
    }
    # 0.0.9 patch 9a: dedup raw type_usage entries against the
    # type_usage_family slot picks. If Selection family is selected,
    # exclude Selection::new from the raw type_usage slot to avoid
    # showing Selection twice (once as family, once as singular).
    covered_family_outers = set()
    covered_family_inners = set()

    def _record_family_coverage(family_entry):
        # entry["pattern"] is "type_usage_family:outer:X" or
        # "type_usage_family:inner:Y".
        body = family_entry["pattern"].split(":", 1)[1]
        if body.startswith("outer:"):
            covered_family_outers.add(body.split(":", 1)[1])
        elif body.startswith("inner:"):
            covered_family_inners.add(body.split(":", 1)[1])

    def _type_usage_covered_by_family(type_usage_entry):
        body = type_usage_entry["pattern"].split(":", 1)[1]
        if "::" not in body:
            return False
        outer, _, inner = body.partition("::")
        return outer in covered_family_outers or inner in covered_family_inners

    selected = []
    for kind in ("trait_impl", "derive", "type_usage_family",
                 "type_usage", "reg_macro"):
        quota = quotas[kind]
        if quota <= 0:
            continue
        candidates = by_kind[kind]
        if kind == "type_usage":
            candidates = [
                e for e in candidates if not _type_usage_covered_by_family(e)
            ]
        picked = candidates[:quota]
        selected.extend(picked)
        if kind == "type_usage_family":
            for entry in picked:
                _record_family_coverage(entry)
    # If underfilled (some kind had no picks), backfill from trait_impl pool.
    if len(selected) < n:
        chosen_patterns = {e["pattern"] for e in selected}
        for entry in by_kind["trait_impl"][quotas["trait_impl"]:]:
            if entry["pattern"] in chosen_patterns:
                continue
            selected.append(entry)
            if len(selected) >= n:
                break
    selected.sort(
        key=lambda x: (_PLURAL_KIND_PRIORITY.get(x["kind"], 99), -x["count"]))
    out = []
    for entry in selected[:n]:
        inst, spans = _instance_for_kind(
            entry["kind"], entry["pattern"].split(":", 1)[1], facts)
        if inst is None:
            continue
        out.append({
            "kind": entry["kind"],
            "pattern": entry["pattern"],
            "instance": inst,
            "all_spans": spans,
            "fallback_reason": (
                f"Surfaced via per-crate top-pattern aggregation; source crate "
                f"`{entry['source_crate']}` ({entry['count']} instances of "
                f"`{entry['pattern']}` in that crate)."
            ),
            "source_crate": entry["source_crate"],
            "count": entry["count"],
        })
    return out


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
    # 0.0.8 patch 8c: workspace_types is the set of struct / enum / union /
    # type-alias names declared in the workspace; the outer of a type_usage
    # pattern is workspace-defined when it appears here.
    workspace_types = {t["name"] for t in facts.get("types", []) if t.get("name")}
    # 0.0.8 patch 8c: external lynchpins are type_usage histogram entries
    # whose combined name appears above _LYNCHPIN_USAGE_MIN (tokio's task::spawn
    # + Poll::Ready when scanning an app using tokio; nothing for a standalone
    # workspace whose internal type_usage frequencies are all workspace-
    # defined). Generic outers (Default / Display / From / etc.) are excluded
    # so the lynchpin set surfaces only architectural-shape patterns.
    lynchpins = set()
    for entry in histogram:
        dom = entry.get("pattern", "")
        if not dom.startswith("type_usage:"):
            continue
        name = dom.split(":", 1)[1]
        if _is_generic_pattern("type_usage", name):
            continue
        outer = name.split("::", 1)[0]
        if outer in workspace_types:
            continue
        if entry.get("count", 0) >= _LYNCHPIN_USAGE_MIN:
            lynchpins.add(name)
    # 0.0.7 patch 7b: per-crate top-pattern aggregation runs FIRST. If it
    # surfaces any candidates (workspace has multiple crates each with a
    # workspace-defined non-generic pattern), return those as the plural
    # protagonist list. If empty (cosmic-epoch-style submodule aggregator
    # with no workspace-defined patterns at all), fall through to the
    # single-pick logic from 6a/6b below.
    per_crate_counts = _per_crate_top_patterns(
        facts, workspace_traits, macro_defs_idx, workspace_types, lynchpins)
    plural_picks = _aggregate_per_crate_picks(per_crate_counts, facts, _PLURAL_N)
    if plural_picks:
        return plural_picks
    leader_dom = histogram[0]["pattern"]
    leader_kind, _, leader_name = leader_dom.partition(":")
    leader_inst, leader_spans = _instance_for_kind(leader_kind, leader_name, facts)
    leader_is_generic = _is_generic_pattern(leader_kind, leader_name)
    leader_is_workspace = _is_workspace_defined(
        leader_kind, leader_name, workspace_traits, macro_defs_idx,
        workspace_types, lynchpins)
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
    # Two-pass walk. Priority: trait_impl > derive > type_usage_family >
    # type_usage > reg_macro. Pass 1 restricts to workspace-defined
    # patterns; pass 2 (only runs if pass 1 finds nothing) falls back to
    # non-workspace patterns. Generic patterns are skipped in both passes
    # per Patch dd + 6a + 8c + 9a.
    priority_order = ("trait_impl", "derive", "type_usage_family",
                      "type_usage", "reg_macro")

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
                    kind, name, workspace_traits, macro_defs_idx,
                    workspace_types, lynchpins):
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
          "architectural orientation for a specific topic, run rust_recon "
          "against the sub-workspace of interest.", ""]
    for name in sorted(fp["per_crate"]):
        c = fp["per_crate"][name]
        ideps = [d for d in c["deps"] if d in fp["per_crate"]]
        dep_str = f" -> depends on: {', '.join(ideps)}" if ideps else ""
        L.append(f"- **{name}** ({c['dir']}/, {c['loc']} LoC, "
                 f"{c['n_impls']} impls, {c['n_types']} types){dep_str}")
    L.append("")
    L.append("**[AGENT]** Pick the member whose architectural pattern "
             "you want to trace, then run rust_recon against that "
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

    # 5. worked slices (plural; surfaced via per-crate top-pattern aggregation
    # under 0.0.7 patch 7b + 7c).
    L += ["## 5. Worked slices - the authoring templates", ""]
    if cands:
        n_cands = len(cands)
        L.append(f"**{n_cands} protagonist pattern{'s' if n_cands != 1 else ''}** "
                 f"surfaced via per-crate top-pattern aggregation (0.0.7 patch 7b + "
                 f"7c). Trace each across crate boundaries; the cross-pattern "
                 f"composition / shared-seam view is at the end of this section.")
        L.append("")
        L.append("Pattern shape claude.ai's framework didn't catch: most workspaces "
                 "have plural architectural patterns. method.md's 'trace one, not "
                 "three' was claude.ai's hypothesis stated as doctrine. Empirical "
                 "evidence (9-10 of 10 deployment targets have 4-6 architectural "
                 "patterns per manual ground-truth audit) refutes it as a default. "
                 "Plurality is the default; single-protagonist is the exception. See "
                 "notes/rust_recon/methodology_findings.md in the consuming KB for "
                 "the empirical record.")
        L.append("")
        for idx, c in enumerate(cands, 1):
            dom = c["pattern"]
            inst = c["instance"]
            source_crate = c.get("source_crate", "?")
            count = c.get("count", 0)
            L.append(f"### 5.{idx} Worked slice: **`{dom}`**")
            L.append("")
            L.append(f"Source crate: `{source_crate}` ({count} instances of this "
                     f"pattern in that crate, per the per-crate aggregation that "
                     f"surfaced it).")
            if c["kind"] == "trait_impl":
                L.append(f"Seed instance: `impl {dom.split(':')[1]} for "
                         f"{inst.get('type')}` - {sp(inst)}.")
            else:
                L.append(f"Seed instance - {sp(inst)}.")
            if c.get("fallback_reason"):
                L.append("")
                L.append(f"**Pick rationale:** {c['fallback_reason']}")
            L.append("")
            L.append(f"**[AGENT]** Trace this instance of `{dom}` across every "
                     f"crate boundary it touches:")
            L += ["- **what** it does: inputs / outputs / state + environment "
                  "changes (load-bearing - read the impl body in source).",
                  "- **where** it plugs in: how it is registered and invoked "
                  "(follow the registration path; if it goes through a macro, "
                  "that is a guardrail - see S6).",
                  "- **why** it is shaped this way: from doc-comments only, "
                  "else `why: unverified`.",
                  "- stop honestly at each seam (S3) with `UNRESOLVED: <what "
                  "you looked for>, <what you ran>`."]
            if c.get("all_spans"):
                L.append("")
                L.append("Other instances of this pattern (open any to compare): "
                         + ", ".join(f"`{s}`" for s in c["all_spans"][:15])
                         + (" ..." if len(c["all_spans"]) > 15 else ""))
            L.append("")
        L.append(f"### 5.{n_cands + 1} How these patterns connect")
        L.append("")
        L.append(f"**[AGENT]** The {n_cands} patterns above are not independent. "
                 f"After tracing each individually, identify the composition: "
                 f"which seams from S3 do they cross together? Which types from "
                 f"S2 flow between them (e.g. one pattern's output is another's "
                 f"input)? Which lifecycle dependencies exist (one pattern's "
                 f"setup precedes another's use)? This composition view is the "
                 f"architecture the workspace's authors hold in their head; "
                 f"surface it explicitly here, anchored to the spans you opened "
                 f"in 5.1 through 5.{n_cands}.")
    else:
        L.append("**[AGENT]** No structural patterns surfaced from per-crate "
                 f"aggregation (mode: {sel['mode']}). Inspect the histogram "
                 "appendix and pick manually; the workspace shape may be unusual "
                 "(submodule aggregator, no workspace-defined traits, etc.).")
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

    # 7. pattern-authoring guides (plural; mirrors S5's worked slices)
    L += ["## 7. Pattern-authoring guides", ""]
    if cands:
        L.append("One authoring checklist per protagonist pattern. Each guide "
                 "lists the minimal steps to add a NEW instance of that pattern, "
                 "drawing from the worked slice in the corresponding S5.N section.")
        L.append("")
        for idx, c in enumerate(cands, 1):
            dom = c["pattern"]
            L.append(f"### 7.{idx} Authoring guide: **`{dom}`**")
            L.append("")
            L.append(f"**[AGENT]** From the trait / type definitions in S2 and "
                     f"the worked slice in S5.{idx}, write the minimal checklist "
                     f"to author a NEW instance of `{dom}`: which trait to "
                     f"implement (or macro to invoke), which methods / args are "
                     f"required (read the trait def in source), how to register "
                     f"the new instance (the registration path from S5.{idx}), "
                     f"and which seams (S6) a new instance must respect. Anchor "
                     f"each step to a span.")
            L.append("")
        L.append(f"### 7.{len(cands) + 1} Cross-pattern shared scaffolding")
        L.append("")
        L.append(f"**[AGENT]** Collect duplicated steps across the {len(cands)} "
                 f"authoring guides above. Patterns that share a registration "
                 f"path (default_context.rs, a common builder), a trait import "
                 f"(common workspace prelude), or a seam compliance step (PipelineData "
                 f"shape, Selection invariants, etc.) deserve a 'this applies to all "
                 f"protagonists' note. The shared scaffolding is the authoring "
                 f"context a contributor learns once and reuses across patterns.")
    else:
        L.append("**[AGENT]** From the trait / struct definitions in S2 and the "
                 "worked slice in S5, write the minimal checklist to author a "
                 "NEW instance of the dominant pattern.")
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
