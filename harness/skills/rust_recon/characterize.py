"""characterize.py - phase 1 of the orientation pipeline.

Walks a Rust workspace, builds the crate dependency graph, scans every `.rs` file for facts
(via rustscan), computes the structural fingerprint, and selects the trace mode. Writes
`fingerprint.json` (the characterization, written FIRST so the chosen mode is auditable
before any costly trace) and `facts.json` (the exhaustive fact table consumed by emit.py).

Pure stdlib. `tomllib` (3.11+) parses Cargo.toml; union-find computes disjoint components.
No clustering / community detection: region cutting (when needed) is decided by disjoint
components and, within a monolithic component, by seam density - never by graph modularity.

Thresholds are DECLARED ASSUMPTIONS, surfaced in the fingerprint and overridable via env
vars. They are defensible defaults, not constants calibrated against real repos (which this
build environment cannot run).
"""

from __future__ import annotations
import json
import os
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import config  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover (Python < 3.11)
    tomllib = None

# --- Declared, overridable thresholds (surfaced in the fingerprint) ---------------------
# 0.0.30: calibration constants moved to calibration.toml; loaded via
# config.py. Env vars (ORIENT_*) still override the TOML defaults.
DOMINANCE_SHARE = config.float_param(
    "ORIENT_DOMINANCE_SHARE", "mode", "dominance_share", default=0.45)
COEQUAL_TOPK = config.int_param(
    "ORIENT_COEQUAL_TOPK", "mode", "coequal_topk", default=4)
COEQUAL_SHARE = config.float_param(
    "ORIENT_COEQUAL_SHARE", "mode", "coequal_share", default=0.60)
AMBIGUOUS_BAND = config.float_param(
    "ORIENT_AMBIGUOUS_BAND", "mode", "ambiguous_band", default=0.07)
SEAM_DENSE_PER_KLOC = config.float_param(
    "ORIENT_SEAM_DENSE_PER_KLOC", "mode", "seam_dense_per_kloc", default=4.0)


# 0.0.8 patch 8b: src-only filter for type_usages. Test / bench /
# example files are excluded from the architectural type-usage
# aggregation - those files demonstrate canonical usage and belong to
# the 0.0.10 example-mining scope. Path segment match against the
# excluded directory names; works for both per-crate layouts
# (`crates/<X>/tests/...`) and workspace-top-level layouts
# (`tests/...` as nushell uses).
_EXCLUDED_DIR_SEGMENTS = config.tuple_param(
    "filters", "excluded_dir_segments",
    default=("tests", "benches", "examples"))

# 0.0.28: filters for method_ref synthesis from AST scanner output.
# Tighter than emit.py's _GENERIC_AUTO_TYPES + _GENERIC_INNER_METHODS
# because method_ref entries grow pattern_metrics size (the coverage
# denominator); admitting noise broadly would degrade the coverage
# metric without surfacing architectural protagonists.
#
# Self:: refs are inside impl blocks and refer to the impl target;
# they're not architectural signal at the workspace level.
#
# Generic outer trait shells (Default, From, etc.) are universally
# implemented + universally referenced; they're not workspace-
# specific architectural patterns. Matches emit.py's
# _GENERIC_AUTO_TYPES exactly.
#
# Generic inner method names (new, default, from, fmt, clone, etc.)
# are constructor / formatter conventions, not architectural verbs.
# iced's `update` and similar workspace-specific verbs survive this
# filter. Matches emit.py's _GENERIC_INNER_METHODS exactly.
_METHOD_REF_OUTER_SKIP = config.frozenset_param(
    "filters", "method_ref_outer_skip")
_METHOD_REF_INNER_SKIP = config.frozenset_param(
    "filters", "method_ref_inner_skip")

# 0.0.28: minimum distinct workspace-defined-pub outers for a
# method_ref family to qualify as a workspace-wide protagonist.
# 3 is the working threshold: 1-2 outers indicates per-Type
# methodology; 3+ outers across multiple Type definitions indicates
# the method name is the architectural concept the workspace's
# external API uses as a hook (iced's update + view + draw etc.).
# 0.0.30: moved to calibration.toml [picker.family].
_METHOD_REF_FAMILY_MIN_OUTERS = config.int_param(
    "ORIENT_METHOD_REF_FAMILY_MIN_OUTERS",
    "picker", "family", "method_ref_min_outers", default=3)


def _is_src_file(rel: str) -> bool:
    parts = rel.replace("\\", "/").split("/")
    return not any(seg in _EXCLUDED_DIR_SEGMENTS for seg in parts)


def find_crates(root: Path):
    """Return (crates, workspace_roots). crates: name -> {dir, deps,
    has_bin, has_lib, keywords, categories, description}.

    0.0.13 patch 13d + 13e + 13f + 0.0.15 patch 15a: each crate's
    record carries the raw bin / lib presence signals. The 4-bucket
    use-classification (end_use / dev_use / end_with_dev_use /
    dev_with_end_use) is computed downstream in main() after
    pattern_metrics is built - the cross-crate is_pub usage signal
    is load-bearing per the_user 2026-06-03 ('lib.rs / [lib] with
    significant is_pub may suggest dev_use or dev_with_end_use').
    Package metadata (keywords, categories, description) is
    captured for downstream consumption."""
    crates = {}
    workspace_roots = []
    for cargo in root.rglob("Cargo.toml"):
        if "target" in cargo.parts:
            continue
        try:
            data = tomllib.loads(cargo.read_text(encoding="utf-8", errors="replace"))
        except Exception:
            continue
        if "workspace" in data:
            workspace_roots.append(str(cargo.parent.relative_to(root)))
        pkg = data.get("package")
        if isinstance(pkg, dict) and "name" in pkg:
            name = pkg["name"]
            deps = set()
            for sect in ("dependencies", "dev-dependencies", "build-dependencies"):
                d = data.get(sect, {})
                if isinstance(d, dict):
                    deps.update(d.keys())
            crate_dir = cargo.parent
            # 13d: detect bin presence from [[bin]] OR src/main.rs OR
            # src/bin/ directory existence.
            has_bin_entry = bool(data.get("bin"))
            has_main_rs = (crate_dir / "src" / "main.rs").exists()
            has_bin_dir = (crate_dir / "src" / "bin").is_dir()
            has_bin = has_bin_entry or has_main_rs or has_bin_dir
            # Lib presence: explicit [lib] table OR src/lib.rs file.
            # Cargo defaults to a lib if src/lib.rs exists and no
            # explicit lib config disables it.
            has_lib_entry = bool(data.get("lib"))
            has_lib_rs = (crate_dir / "src" / "lib.rs").exists()
            has_lib = has_lib_entry or has_lib_rs
            crates[name] = {
                "dir": str(cargo.parent.relative_to(root)) or ".",
                "deps": sorted(deps),
                "has_bin": has_bin,
                "has_lib": has_lib,
                # 13f: package metadata for downstream consumption.
                "keywords": pkg.get("keywords", []) or [],
                "categories": pkg.get("categories", []) or [],
                "description": (pkg.get("description") or "").strip(),
            }
    return crates, sorted(set(workspace_roots))


# 0.0.15 patch 15a: minimum cross-crate is_pub usage to flip a crate
# with BOTH bin and lib from end_with_dev_use to dev_with_end_use.
# Default 30 was chosen empirically against the 10-target probe:
# helix-term (16) and helix-loader (12) have small lib pub-cross-crate
# usage because their libs are internal organization rather than a
# real public library API; helix-core (401) and helix-view (430) are
# genuine library crates - but they're has_lib-only so the threshold
# doesn't apply. The threshold's only effect is on has-both-bin-and-
# lib crates, where it distinguishes 'the lib is the deliverable'
# (gitoxide pattern; pub_inter_count would be in the hundreds because
# other workspace crates depend on the lib) from 'the bin is the
# deliverable and the lib is auxiliary' (nushell `nu_plugin_*`,
# helix-term pattern). Env-tunable.
#
# Tech-debt the_user 2026-06-03: this is a fixed absolute count
# rather than a ratio. Doesn't scale with workspace size - a small
# workspace's 'significant cross-crate usage' might be 5; a huge
# workspace's might be 500. Long-term we want a ratio (e.g.
# pub_inter_count / max_pub_inter_count_in_workspace, or
# pub_inter_count / pub_count). Tracked in
# notes/rust_recon/debt.md.
_DEV_WITH_END_THRESHOLD = config.int_param(
    "ORIENT_DEV_WITH_END_USE_THRESHOLD",
    "classification", "dev_with_end_threshold", default=30)


def _classify_crate_use(name: str, info: dict,
                        pattern_metrics: dict) -> str:
    """0.0.15 patch 15a: per-crate 4-bucket use-classification.

    Returns one of: 'end_use', 'dev_use', 'end_with_dev_use',
    'dev_with_end_use'.

    Tech-debt the_user 2026-06-03: a 5th bucket 'dev_and_end_use'
    is missing for cases where both bin and lib are co-equally
    primary deliverables (gitoxide is the canonical example - its
    docs say 'there are two primary ways to use gitoxide'). The
    rubric below collapses such cases into dev_with_end_use which
    preserves 'lib at least as primary' but loses 'bin equally
    primary'. The_user 2026-06-03: 'it will bite us'. Will surface
    when a gitoxide-class workspace is probed or when downstream
    consumership-aware emit prompts depend on the distinction.
    Tracked in notes/rust_recon/debt.md.

    Rubric (the_user 2026-06-03):
    - has_lib and not has_bin -> dev_use.
    - has_bin and not has_lib -> end_use.
    - has_bin and has_lib: weigh by cross-crate is_pub usage of
      patterns defined in this crate. The_user 2026-06-03 framing:
      '[bin] can be a signal that may be either end_use or
      end_with_dev_use, or it could just be some demo (irrelevant).
      lib.rs / [lib] with significant is_pub may suggest dev_use or
      dev_with_end_use'. If the lib's pub items are used cross-crate
      at or above _DEV_WITH_END_THRESHOLD, the lib is the primary
      deliverable and the bin is auxiliary (dev_with_end_use, the
      gitoxide pattern). Otherwise the bin is the primary deliverable
      and the lib is internal organization (end_with_dev_use, the
      nushell `nu` / helix `helix-term` pattern).
    - neither -> dev_use (fallback, unusual)."""
    has_bin = info.get("has_bin", False)
    has_lib = info.get("has_lib", False)
    if has_lib and not has_bin:
        return "dev_use"
    if has_bin and not has_lib:
        return "end_use"
    if not has_bin and not has_lib:
        return "dev_use"
    # has_bin and has_lib: multi-signal weighing.
    pub_inter_count = sum(
        (m.get("inter_count", 0) or 0)
        for pattern, m in pattern_metrics.items()
        if m.get("defining_crate") == name and m.get("is_pub")
    )
    if pub_inter_count >= _DEV_WITH_END_THRESHOLD:
        return "dev_with_end_use"
    return "end_with_dev_use"


def _classify_workspace_use(crates: dict,
                            pattern_metrics: dict) -> dict:
    """0.0.13 patch 13e + 0.0.15 patch 15a: aggregate per-crate use-
    classification into a workspace-level 4-bucket assignment +
    per-crate breakdown.

    Excludes example / bench / fuzz / xtask / tools / ci / build /
    scripts crates from the workspace classification - they're
    scaffolding, not the workspace's primary purpose. A workspace
    with many demo example crates is still a dev_use workspace if
    its non-example crates are all libraries.

    Returns {workspace: 'end_use' | 'dev_use' | 'end_with_dev_use'
    | 'dev_with_end_use', per_crate: {...}, buckets: {...},
    scaffolding_crates: [...], reasoning: str}."""
    def _is_scaffolding(name: str, info: dict) -> bool:
        # Check the crate's directory path - example/bench/fuzz/tests/
        # tools/ci dirs are scaffolding by convention. Also check name
        # patterns.
        # 0.0.13 patch 13g: expanded scaffolding detection per the_user
        # 2026-06-03 observation that bevy / rustls / tokio were
        # misclassified as hybrid because their build/CI/bench tools
        # leaked into the 'app' bucket. Added: tools/, ci/, bench/,
        # scripts/, build-* / ci_* prefixes, -bench / _bench suffixes,
        # bare 'xtask' / 'ci' / 'bogo' as standalone names.
        d = (info.get("dir") or "").lower()
        path_segments = (
            "/examples/", "examples/", "/example/", "example/",
            "/benches/", "benches/", "/bench/", "bench/",
            "/fuzz/", "fuzz/",
            "/tests/", "tests/", "/test/", "test/",
            "/xtask/", "xtask/",
            "/tools/", "tools/",
            "/ci/", "ci/",
            "/scripts/", "scripts/",
            "/build/", "build/",
        )
        if any(seg in d for seg in path_segments) or d in (
                "xtask", "ci", "bogo", "build", "tools", "tests",
                "scripts", "examples", "benches", "bench", "fuzz"):
            return True
        nm = name.lower()
        suffix_patterns = (
            "_example", "_demo", "_fuzz", "_test", "_tests",
            "_bench", "_benches", "_xtask",
            "-example", "-demo", "-fuzz", "-test", "-tests",
            "-bench", "-benches", "-xtask",
        )
        if any(suf in nm for suf in suffix_patterns):
            return True
        prefix_patterns = (
            "example_", "example-", "demo_", "demo-",
            "build_", "build-", "build_templated", "ci_", "ci-",
            "tests-", "test-", "bench-", "bench_",
            "fuzz_", "fuzz-",
            "rustls-bench", "rustls-ci-bench",
            "export-content", "export_content",
            "tests-integration",
        )
        if any(nm.startswith(p) for p in prefix_patterns):
            return True
        # Standalone names that are conventionally scaffolding.
        if nm in ("xtask", "ci", "bogo", "build", "tools",
                  "example-showcase", "export-content",
                  "tests-integration"):
            return True
        return False

    per_crate_class = {}
    scaffolding_crates = []
    for name, info in crates.items():
        if _is_scaffolding(name, info):
            scaffolding_crates.append(name)
            continue
        per_crate_class[name] = _classify_crate_use(
            name, info, pattern_metrics)
    buckets = {
        "end_use": 0, "dev_use": 0,
        "end_with_dev_use": 0, "dev_with_end_use": 0,
    }
    for cls in per_crate_class.values():
        if cls in buckets:
            buckets[cls] += 1
    n_total = sum(buckets.values())
    n_scaf = len(scaffolding_crates)
    # 0.0.15 patch 15a workspace aggregation rules.
    if n_total == 0:
        workspace = "dev_use"
        reasoning = "no primary crates after scaffolding exclusion"
    elif buckets["dev_use"] == n_total:
        workspace = "dev_use"
        reasoning = f"all {n_total} primary crates are pure dev_use libraries"
    elif buckets["end_use"] == n_total:
        workspace = "end_use"
        reasoning = f"all {n_total} primary crates are pure end_use binaries"
    elif buckets["end_with_dev_use"] > 0 or buckets["end_use"] > 0:
        workspace = "end_with_dev_use"
        reasoning = (
            f"primary crates: end_use={buckets['end_use']} "
            f"end_with_dev_use={buckets['end_with_dev_use']} "
            f"dev_with_end_use={buckets['dev_with_end_use']} "
            f"dev_use={buckets['dev_use']}; the workspace ships an "
            f"end-user product with the libraries composing it"
        )
    elif buckets["dev_with_end_use"] > 0:
        workspace = "dev_with_end_use"
        reasoning = (
            f"primary crates: dev_use={buckets['dev_use']} "
            f"dev_with_end_use={buckets['dev_with_end_use']}; the "
            f"workspace's primary deliverable is a library with a "
            f"CLI auxiliary (gitoxide pattern)"
        )
    else:
        workspace = "dev_use"
        reasoning = "fallback default"
    if n_scaf:
        reasoning += f" ({n_scaf} scaffolding crates excluded)"
    return {
        "workspace": workspace,
        "per_crate": per_crate_class,
        "buckets": buckets,
        "scaffolding_crates": sorted(scaffolding_crates),
        "reasoning": reasoning,
    }


def classify_workspace_shape(fp_partial: dict, all_facts: dict) -> dict:
    """0.0.8 patch 8e: heuristic shape classifier. Replaces the 8d
    annotation-based detection (the_user 2026-06-03: 'we should not
    need to alter any source-code. the point of the heuristics is to
    analyze correctly, without intervention').

    Reads the partial fingerprint (per_crate + pattern_histogram already
    computed) plus all_facts and returns a shape label + signals + a
    routable hint. Shapes emerging from empirical 10-target probe:

    - container: central crate's dominant kind is not type_usage (so it's
      shared infra, not an architectural framework), AND per-crate top
      kinds are diverse (dispersion > 0.3), AND patterns are mostly
      crate-isolated (uniqueness > 0.7). sourcetrait_common is the
      canonical example - shared testing infra at the hub + topical
      libraries each with their own dominant kind.
    - tight_framework: central crate is type_usage-dominant + patterns
      share heavily across crates (uniqueness < 0.2) + crates align
      on the same kind (dispersion < 0.1). bevy / iced / nushell.
    - framework_with_users: central type_usage framework crate + many
      thin client crates around it (high leaf_ratio + high hub_centrality).
      tokio / rustls / libcosmic / helix.
    - framework_product: central type_usage framework + moderate pattern
      sharing. The middle ground.
    - submodule_aggregator: low dispersion (<0.1) + central kind is not
      type_usage. cosmic-epoch (config crate as central, all members
      type_usage-aligned).
    - mixed: signals don't match a known shape. Emit as normal
      orientation; flag for follow-up.

    The container shape is the only one that routes to a different emit
    path (emit_container_routing). All other shapes use the normal
    emit_orientation with the shape label surfaced for the agent's
    awareness."""
    per_crate = fp_partial.get("per_crate", {})
    n_crates = len(per_crate)

    if n_crates == 0:
        return {"shape": "empty", "signals": {},
                "reasoning": "no crates found in the workspace"}
    if n_crates == 1:
        return {"shape": "monolith", "signals": {"n_crates": 1},
                "reasoning": "single-crate workspace"}

    signals = _compute_shape_signals(fp_partial, all_facts)

    central_kind = signals["central_kind"]
    uniqueness = signals["uniqueness_ratio"]
    dispersion = signals["kind_dominance_dispersion"]
    leaf_ratio = signals["leaf_ratio"]
    hub_centrality = signals["hub_centrality"]

    # Container: central is shared infra (not architectural framework),
    # crates have diverse dominant kinds, patterns are crate-isolated.
    if (central_kind not in ("type_usage", None)
            and uniqueness > 0.7
            and dispersion > 0.3):
        return {
            "shape": "container",
            "signals": signals,
            "reasoning": (
                f"central crate's dominant kind is `{central_kind}` "
                f"(not the architectural type_usage axis) - the hub is "
                f"shared infrastructure, not a framework; per-crate "
                f"top kinds are diverse (dispersion {dispersion:.3f}) "
                f"indicating each member is its own topical library; "
                f"patterns are mostly crate-isolated (uniqueness "
                f"{uniqueness:.3f}). Each member should be probed "
                f"individually for its architectural pattern."
            ),
        }

    # Submodule aggregator: low dispersion (members aligned) + central
    # crate not type_usage. cosmic-epoch's config-crate-as-central case.
    if dispersion < 0.1 and central_kind not in ("type_usage", None):
        return {
            "shape": "submodule_aggregator",
            "signals": signals,
            "reasoning": (
                f"per-crate top kinds align (dispersion {dispersion:.3f}); "
                f"central crate is `{central_kind}`-dominant rather than "
                f"type_usage; structural shape suggests a config / "
                f"versioning aggregator with many parallel topical "
                f"submodules."
            ),
        }

    # Tight framework: central is type_usage + heavy pattern sharing +
    # aligned kinds. bevy / iced / nushell.
    if (central_kind == "type_usage"
            and uniqueness < 0.2
            and dispersion < 0.1):
        return {
            "shape": "tight_framework",
            "signals": signals,
            "reasoning": (
                f"central type_usage framework crate; patterns share "
                f"heavily across crates (uniqueness {uniqueness:.3f}); "
                f"crates align on the same dominant kind (dispersion "
                f"{dispersion:.3f}). Trace the central crate first."
            ),
        }

    # Framework with users: central is type_usage + many thin clients.
    # tokio / rustls / libcosmic.
    if (central_kind == "type_usage"
            and leaf_ratio > 0.5
            and hub_centrality > 0.7):
        return {
            "shape": "framework_with_users",
            "signals": signals,
            "reasoning": (
                f"central type_usage framework crate; many leaf "
                f"crates (leaf_ratio {leaf_ratio:.3f}); high hub "
                f"centrality ({hub_centrality:.3f}) indicates a "
                f"framework + many independent client crates."
            ),
        }

    # Framework product: central type_usage + moderate everything.
    # helix is the canonical example.
    if central_kind == "type_usage":
        return {
            "shape": "framework_product",
            "signals": signals,
            "reasoning": (
                f"central type_usage framework crate; moderate pattern "
                f"sharing (uniqueness {uniqueness:.3f}); typical layered "
                f"product workspace shape."
            ),
        }

    # Default: signals don't match a known shape.
    return {
        "shape": "mixed",
        "signals": signals,
        "reasoning": (
            f"signals don't match a known shape: central_kind="
            f"{central_kind}, uniqueness={uniqueness:.3f}, dispersion="
            f"{dispersion:.3f}, leaf_ratio={leaf_ratio:.3f}, "
            f"hub_centrality={hub_centrality:.3f}. Emit as standard "
            f"orientation; the shape may surface during follow-up "
            f"analysis."
        ),
    }


def _compute_shape_signals(fp_partial: dict, all_facts: dict) -> dict:
    """Compute the structural signals fed into classify_workspace_shape.
    Pure-functional given (fp_partial, all_facts); no external state.
    See classify_workspace_shape's docstring for signal semantics."""
    per_crate = fp_partial.get("per_crate", {})
    crate_names = sorted(per_crate.keys())
    n_crates = max(1, len(crate_names))

    pattern_to_crates = defaultdict(set)
    for it in all_facts.get("impls", []):
        if it.get("trait") and not it.get("cfg_gated"):
            pattern_to_crates[f"trait_impl:{it['trait']}"].add(
                it.get("crate"))
    for d in all_facts.get("derives", []):
        if d.get("trait"):
            pattern_to_crates[f"derive:{d['trait']}"].add(d.get("crate"))
    for m in all_facts.get("macros", []):
        kind = m.get("kind")
        nm = m.get("name")
        if not nm:
            continue
        if kind == "attr_macro":
            pattern_to_crates[f"attr_macro:{nm}"].add(m.get("crate"))
        elif kind == "macro_invocation":
            pattern_to_crates[f"reg_macro:{nm}"].add(m.get("crate"))
    for tu in all_facts.get("type_usages", []):
        if tu.get("name"):
            pattern_to_crates[f"type_usage:{tu['name']}"].add(tu.get("crate"))
    unique_patterns = len(pattern_to_crates)
    single_crate_patterns = sum(
        1 for s in pattern_to_crates.values() if len(s) == 1)
    uniqueness_ratio = (single_crate_patterns / unique_patterns
                        if unique_patterns else 0.0)

    per_crate_kinds = defaultdict(Counter)
    for it in all_facts.get("impls", []):
        if it.get("trait") and not it.get("cfg_gated"):
            per_crate_kinds[it.get("crate")]["trait_impl"] += 1
    for d in all_facts.get("derives", []):
        per_crate_kinds[d.get("crate")]["derive"] += 1
    for m in all_facts.get("macros", []):
        kind = m.get("kind")
        if kind == "attr_macro":
            per_crate_kinds[m.get("crate")]["attr_macro"] += 1
        elif kind == "macro_invocation":
            per_crate_kinds[m.get("crate")]["reg_macro"] += 1
    for tu in all_facts.get("type_usages", []):
        per_crate_kinds[tu.get("crate")]["type_usage"] += 1
    per_crate_top_kind = {}
    for crate, counter in per_crate_kinds.items():
        if counter:
            per_crate_top_kind[crate] = counter.most_common(1)[0][0]
    distinct_top_kinds = len(set(per_crate_top_kind.values()))
    kind_dominance_dispersion = (
        distinct_top_kinds / max(1, len(per_crate_top_kind)))

    workspace_crate_set = set(crate_names)
    dependent_count = Counter()
    for name, info in per_crate.items():
        for dep in info.get("deps", []):
            if dep in workspace_crate_set:
                dependent_count[dep] += 1
    leaf_crates = sum(
        1 for name in crate_names if dependent_count[name] == 0)
    leaf_ratio = leaf_crates / n_crates
    max_dependents = max(dependent_count.values()) if dependent_count else 0
    hub_centrality = max_dependents / n_crates

    central_crate = (dependent_count.most_common(1)[0][0]
                     if dependent_count else None)
    central_kind = (per_crate_top_kind.get(central_crate)
                    if central_crate else None)

    return {
        "uniqueness_ratio": round(uniqueness_ratio, 3),
        "kind_dominance_dispersion": round(kind_dominance_dispersion, 3),
        "leaf_ratio": round(leaf_ratio, 3),
        "hub_centrality": round(hub_centrality, 3),
        "central_crate": central_crate,
        "central_kind": central_kind,
        "n_crates": len(crate_names),
        "unique_patterns": unique_patterns,
        "single_crate_patterns": single_crate_patterns,
        "distinct_top_kinds": distinct_top_kinds,
    }


class UnionFind:
    def __init__(self, items):
        self.parent = {x: x for x in items}

    def find(self, x):
        while self.parent[x] != x:
            self.parent[x] = self.parent[self.parent[x]]
            x = self.parent[x]
        return x

    def union(self, a, b):
        ra, rb = self.find(a), self.find(b)
        if ra != rb:
            self.parent[ra] = rb


def components(crates):
    uf = UnionFind(crates.keys())
    internal = set(crates.keys())
    for name, info in crates.items():
        for dep in info["deps"]:
            if dep in internal:
                uf.union(name, dep)
    groups = defaultdict(list)
    for name in crates:
        groups[uf.find(name)].append(name)
    return [sorted(g) for g in groups.values()]


# 0.0.25: SLOC normalization helpers per the_user 2026-06-04.
# - Strip block comments `/* ... */` (non-nested via re.DOTALL).
# - Strip line + doc comments `//.*$`, `///.*$`, `//!.*$`.
# - Strip blank lines after comment removal.
# - Strip inline `#[cfg(test)] <item>` blocks (brace-matched).
# Result feeds both SLOC count + rustscan facts: tests are
# meaningless to us so they should not contribute to either.

_BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.DOTALL)
_LINE_COMMENT_RE = re.compile(r"//[^\n]*")


def _strip_cfg_test(src: str) -> str:
    """Remove inline `#[cfg(test)] <item>` regions from source.

    Walks the text, finds each `#[cfg(test)]` attribute, scans
    forward past any additional attributes, then brace-matches the
    following item body and removes the [attribute .. body] range.
    Single-line items without a body (e.g. `#[cfg(test)] use foo;`)
    fall through to the next semicolon.

    Pragmatic: ignores string/comment context (false positives only
    in the vanishingly rare case where `#[cfg(test)]` literally
    appears inside a string). Doesn't handle `#[cfg_attr(test, ...)]`
    or other gated variants - workspaces overwhelmingly use the bare
    form for inline test modules."""
    needle = "#[cfg(test)]"
    out = []
    i = 0
    while True:
        idx = src.find(needle, i)
        if idx < 0:
            out.append(src[i:])
            break
        # Keep everything up to the attribute.
        out.append(src[i:idx])
        # Scan past the attribute + any subsequent attributes / whitespace.
        j = idx + len(needle)
        while j < len(src):
            # Skip whitespace.
            while j < len(src) and src[j] in " \t\n\r":
                j += 1
            # Skip another attribute like `#[derive(...)]` or `#[allow(...)]`.
            if j < len(src) and src[j] == "#" and j + 1 < len(src) and src[j + 1] == "[":
                # Find the matching ].
                depth = 0
                k = j
                while k < len(src):
                    if src[k] == "[":
                        depth += 1
                    elif src[k] == "]":
                        depth -= 1
                        if depth == 0:
                            k += 1
                            break
                    k += 1
                j = k
                continue
            break
        # Now look for the item body. Either next `{` (mod/fn/impl/struct/enum/trait)
        # or next `;` for single-line items (use/const/etc.).
        brace = src.find("{", j)
        semi = src.find(";", j)
        if brace < 0 and semi < 0:
            # Malformed; bail.
            break
        if semi >= 0 and (brace < 0 or semi < brace):
            # Single-line item.
            i = semi + 1
            continue
        # Brace-matched item body.
        depth = 0
        k = brace
        while k < len(src):
            c = src[k]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    k += 1
                    break
            k += 1
        i = k
    return "".join(out)


def _compute_sloc(src: str) -> int:
    """Count SLOC: source lines of code with comments + blanks removed.

    Strips block comments, line/doc comments, then counts non-blank
    lines. Pragmatic: doesn't handle `//` inside strings (rare).
    Apply `_strip_cfg_test` first if you want test-block exclusion;
    SLOC counts whatever non-test source you give it."""
    s = _BLOCK_COMMENT_RE.sub("", src)
    s = _LINE_COMMENT_RE.sub("", s)
    return sum(1 for line in s.splitlines() if line.strip())


_ITEMS_BY_FILE: dict = {}


def _run_scan_items(root: Path, out_dir: Path) -> dict:
    """0.0.34: invoke the syn-based rust_recon `scan items` subcommand
    against the workspace, parse recon_items.json, and return the per-
    file lex+structure facts keyed for downstream merging.

    Binary resolves via PATH ($CARGO_HOME/bin/rust_recon after
    `cargo install --path crates/rust_recon`). If the binary is
    absent or fails, returns an empty dict and prints a warning - the
    picker degrades gracefully with whatever AST signal Phase 2 still
    provides via `scan usages`.
    """
    import subprocess
    try:
        subprocess.run(
            ["rust_recon", "scan", "items", str(root), str(out_dir)],
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        print(
            "[characterize] warning: rust_recon binary not on PATH; "
            "items scan skipped. Install via `cargo install --path "
            "crates/rust_recon` from sourcetrait_empower.",
            file=sys.stderr,
        )
        return {}
    except subprocess.CalledProcessError as e:
        print(
            f"[characterize] warning: rust_recon scan items failed "
            f"({e.returncode}): {e.stderr[:300]}",
            file=sys.stderr,
        )
        return {}
    items_path = out_dir / "recon_items.json"
    if not items_path.is_file():
        return {}
    data = json.loads(items_path.read_text())
    print(
        f"[characterize] items scan: "
        f"{len(data.get('impls', []))} impls + "
        f"{len(data.get('derives', []))} derives + "
        f"{len(data.get('type_usages', []))} type_usages + "
        f"{len(data.get('macros', []))} macros"
    )
    return data


def _build_items_index(items_data: dict) -> dict:
    """0.0.34: bucket the flat workspace-level recon_items.json lists by
    file path so scan_crate() can look up per-file facts via rglob's
    relative-path key. Returns {file: {kind: [...]}} matching the
    rustscan.py per-file output shape.
    """
    by_file: dict = {}
    kinds = (
        "impls",
        "traits",
        "types",
        "fns",
        "uses",
        "macros",
        "derives",
        "macro_defs",
        "mods",
        "type_usages",
        "example_type_usages",
    )
    for kind in kinds:
        for rec in items_data.get(kind, []):
            file = rec.get("file", "")
            if not file:
                continue
            by_file.setdefault(file, {k: [] for k in kinds})[kind].append(rec)
    return by_file


def scan_crate(root: Path, crate_dir: str):
    """Scan all .rs under a crate dir; return aggregated facts + SLOC count.

    0.0.34: rustscan.py retired. Item facts come from the pre-loaded
    _ITEMS_BY_FILE index built from recon_items.json (produced by
    `rust_recon scan items` at workspace level). This function still
    walks .rs files for SLOC compute (cheap, Python-side) and looks
    up per-file facts in the index.

    0.0.25 changes still apply:
    - Skips files in `<crate>/tests/` and `<crate>/benches/` entirely
      (no facts, no SLOC). examples/ unchanged.
    - Strips inline `#[cfg(test)]` blocks before SLOC compute. The
      Rust items walker applies an equivalent cfg(test) item-level
      skip during its own walk.
    - SLOC count via _compute_sloc (no comments, no blanks, no test
      blocks). Field renamed loc -> sloc."""
    agg = {"impls": [], "traits": [], "types": [], "fns": [], "uses": [],
           "macros": [], "derives": [], "macro_defs": [], "type_usages": [],
           "mods": [],
           "example_type_usages": []}
    sloc = 0
    base = root / crate_dir
    for rs in base.rglob("*.rs"):
        if "target" in rs.parts:
            continue
        rel_parts = rs.relative_to(base).parts
        if any(seg in ("tests", "benches") for seg in rel_parts):
            continue
        try:
            src = rs.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        src = _strip_cfg_test(src)
        sloc += _compute_sloc(src)
        rel = str(rs.relative_to(root))
        f = _ITEMS_BY_FILE.get(rel)
        if f is None:
            continue
        agg["impls"] += f.get("impls", [])
        agg["traits"] += f.get("traits", [])
        agg["types"] += f.get("types", [])
        agg["fns"] += f.get("fns", [])
        agg["uses"] += f.get("uses", [])
        agg["macros"] += f.get("macros", [])
        agg["derives"] += f.get("derives", [])
        agg["macro_defs"] += f.get("macro_defs", [])
        agg["mods"] += f.get("mods", [])
        agg["type_usages"] += f.get("type_usages", [])
        agg["example_type_usages"] += f.get("example_type_usages", [])
    agg["sloc"] = sloc
    return agg


def pattern_histogram(all_facts):
    """Group candidate patterns by kind and specific name; return ranked list + by-kind mass.

    Candidate kinds (co-equal - the dominant pattern is decided empirically, NOT assumed to
    be `impl Trait for`):
      trait_impl:<Trait>        one entry per impl of that trait
      derive:<Trait>            one entry per derive of that trait
      attr_macro:<path>         one entry per user attribute-macro application
      reg_macro:<name>          one entry per registration-macro *argument* (call-site count,
                                expansion unverified - confirmed by the rustdoc overlay)
      fn_table:<crate>          free functions clustered in a crate (heuristic)
    """
    patterns = Counter()
    by_kind = Counter()
    reg_macro_calls = Counter()

    for it in all_facts["impls"]:
        if it.get("trait") and not it.get("cfg_gated"):
            patterns[f"trait_impl:{it['trait']}"] += 1
            by_kind["trait_impl"] += 1
    for d in all_facts["derives"]:
        patterns[f"derive:{d['trait']}"] += 1
        by_kind["derive"] += 1
    for m in all_facts["macros"]:
        if m["kind"] == "attr_macro":
            patterns[f"attr_macro:{m['name']}"] += 1
            by_kind["attr_macro"] += 1
        elif m["kind"] == "macro_invocation":
            # Each ident argument is a candidate registered item (call-site signal).
            n_args = max(1, len(m.get("arg_idents", [])))
            patterns[f"reg_macro:{m['name']}"] += n_args
            by_kind["reg_macro"] += n_args
            reg_macro_calls[m["name"]] += 1
    # 0.0.8 patch 8b: type_usage histogram entries. Counts factory-call
    # shapes captured by rustscan's 8a pass (mpsc::channel, Selection::new,
    # PipelineData::Value, etc.). Architectural pattern kind the picker's
    # prior 4-kind taxonomy (trait_impl / derive / attr_macro / reg_macro)
    # could not see. Src-only filter already applied in scan_crate.
    for tu in all_facts.get("type_usages", []):
        patterns[f"type_usage:{tu['name']}"] += 1
        by_kind["type_usage"] += 1

    # Function-table heuristic: a crate with many free functions (brace_depth 0) and few
    # trait impls expresses behaviour as free functions. Reported, flagged heuristic.
    free_fns_by_crate = all_facts.get("_free_fns_by_crate", {})
    for crate, n in free_fns_by_crate.items():
        if n >= 20:
            patterns[f"fn_table:{crate}"] += n
            by_kind["fn_table"] += n

    ranked = patterns.most_common()
    return ranked, dict(by_kind), dict(reg_macro_calls)


def select_mode(ranked, by_kind, workspace_roots, n_components):
    total = sum(c for _, c in ranked) or 1
    top_share = (ranked[0][1] / total) if ranked else 0.0
    second_share = (ranked[1][1] / total) if len(ranked) > 1 else 0.0
    topk_share = sum(c for _, c in ranked[:COEQUAL_TOPK]) / total if ranked else 0.0

    structural_regional = (len(workspace_roots) > 1) or (n_components > 1)

    # Histogram decides mode; structural signals escalate to regional.
    if top_share >= DOMINANCE_SHARE:
        hist_mode = "single_dominant"
    elif topk_share >= COEQUAL_SHARE and top_share < DOMINANCE_SHARE:
        hist_mode = "co_equal_few"
    else:
        hist_mode = "no_dominant"

    mode = "regional" if structural_regional else hist_mode

    # Method-selection-level honesty: report runner-up when near a threshold boundary.
    runner_up = None
    notes = []
    if abs(top_share - DOMINANCE_SHARE) <= AMBIGUOUS_BAND:
        runner_up = "single_dominant" if hist_mode != "single_dominant" else "co_equal_few"
        notes.append(
            f"top_share={top_share:.2f} is within {AMBIGUOUS_BAND} of the dominance "
            f"threshold {DOMINANCE_SHARE}; mode is ambiguous between '{hist_mode}' and "
            f"'{runner_up}'. Confirm against the histogram before tracing."
        )
    if structural_regional and hist_mode != "no_dominant":
        notes.append(
            f"structural signals (workspace_roots={len(workspace_roots)}, "
            f"components={n_components}) forced 'regional', but the histogram alone would "
            f"have chosen '{hist_mode}'. The dominant pattern still holds within regions."
        )

    return {
        "mode": mode,
        "histogram_mode": hist_mode,
        "structural_regional": structural_regional,
        "runner_up": runner_up,
        "top_share": round(top_share, 3),
        "second_share": round(second_share, 3),
        "topk_share": round(topk_share, 3),
        "notes": notes,
    }


def _run_ast_scan(root: Path, out_dir: Path, crates: dict) -> dict:
    """Invoke the syn-based rust_recon binary against the workspace,
    parse scan.json, and return the AST-derived signal data keyed for
    downstream merging.

    Binary resolves via PATH ($CARGO_HOME/bin/rust_recon after
    `cargo install --path crates/rust_recon`). If the binary is
    absent or fails, returns an empty dict and prints a warning -
    the picker degrades gracefully (Frame-class types won't surface
    but the rest of the pipeline works).
    """
    import subprocess
    try:
        subprocess.run(
            ["rust_recon", "scan", "usages", str(root), str(out_dir)],
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        print(
            "[characterize] warning: rust_recon binary not on PATH; "
            "AST signal skipped (Frame-class types won't surface). "
            "Install via `cargo install --path crates/rust_recon` "
            "from the sourcetrait_empower workspace.",
            file=sys.stderr,
        )
        return {"fn_sig_usages": [], "field_usages": [],
                "type_alias_usages": [], "method_ref_usages": []}
    except subprocess.CalledProcessError as e:
        print(
            f"[characterize] warning: rust_recon scan usages failed "
            f"({e.returncode}): {e.stderr[:300]}",
            file=sys.stderr,
        )
        return {"fn_sig_usages": [], "field_usages": [],
                "type_alias_usages": [], "method_ref_usages": []}
    scan_path = out_dir / "recon_usages.json"
    if not scan_path.is_file():
        return {"fn_sig_usages": [], "field_usages": [],
                "type_alias_usages": [], "method_ref_usages": []}
    data = json.loads(scan_path.read_text())
    print(
        f"[characterize] ast scan: "
        f"{len(data.get('ast_fn_sig_usages', []))} fn-sig + "
        f"{len(data.get('ast_field_usages', []))} field + "
        f"{len(data.get('ast_type_alias_usages', []))} type-alias + "
        f"{len(data.get('ast_method_ref_usages', []))} method-ref entries"
    )
    return {
        "fn_sig_usages": data.get("ast_fn_sig_usages", []),
        "field_usages": data.get("ast_field_usages", []),
        "type_alias_usages": data.get("ast_type_alias_usages", []),
        "method_ref_usages": data.get("ast_method_ref_usages", []),
    }


def _resolve_crate_for_file(file_path: str, crate_dirs: dict) -> str:
    """For an AST entry's file path (workspace-relative), find the
    crate it belongs to via longest-prefix-match on crate_dirs."""
    norm = file_path.replace("\\", "/")
    best = None
    best_len = -1
    for name, dir_str in crate_dirs.items():
        d = (dir_str or "").replace("\\", "/").rstrip("/")
        if not d or d == ".":
            if best is None:
                best = name
                best_len = 0
            continue
        prefix = d + "/"
        if norm.startswith(prefix) and len(prefix) > best_len:
            best = name
            best_len = len(prefix)
    return best or ""


def _compute_pattern_metrics(all_facts: dict, ast_facts: dict = None,
                             crates: dict = None) -> dict:
    """0.0.10 patches 10b + 10c + 10d: per-pattern metrics dictionary.
    Key is the pattern name as it appears in pattern_histogram (e.g.
    `trait_impl:Plugin`, `type_usage:Selection::new`, `derive:Component`).
    Value is {defining_crate, intra_count, inter_count, inter_ratio,
    is_pub}.

    Resolves the defining crate by name lookup against the relevant facts
    list: trait_impl + derive resolve to facts['traits']; type_usage
    resolves outer-name to facts['types']; reg_macro + attr_macro resolve
    to facts['macro_defs']. Patterns whose underlying type / trait /
    macro is NOT workspace-declared (external) get defining_crate=None
    and intra/inter counts are skipped (the inter/intra distinction is
    workspace-internal).

    is_pub captures the definition site's visibility from 10a's
    rustscan output. For #[macro_export] detection: the rustscan attrs
    list carries macro_export entries; a macro_def with a co-located
    macro_export attr in the same file is treated as pub regardless of
    syntactic visibility (macro_rules doesn't take `pub` directly)."""
    # 0.0.11 patch 11c: name-collision resolution for the definition
    # lookups. When the same type name is defined in multiple workspace
    # crates (helix's Range in both helix-core and helix-lsp-types),
    # first-seen-wins arbitrarily picks one. Refined heuristic: pick
    # the crate with more impl blocks targeting the type name (the
    # canonical home, with the most methods + trait implementations).
    # For traits + macros, count workspace-wide usages similarly.
    impl_target_count = defaultdict(lambda: defaultdict(int))
    for i in all_facts.get("impls", []):
        type_name = i.get("type")
        crate = i.get("crate")
        if type_name and crate:
            impl_target_count[type_name][crate] += 1

    def _pick_canonical_crate(candidates_by_name, name, counts):
        crates = candidates_by_name.get(name, [])
        if len(crates) <= 1:
            return crates[0] if crates else None
        return max(crates, key=lambda c: counts.get(c, 0))

    type_candidates = defaultdict(list)
    type_visibility = {}
    for t in all_facts.get("types", []):
        name = t.get("name")
        crate = t.get("crate")
        if name and crate:
            if crate not in type_candidates[name]:
                type_candidates[name].append(crate)
            key = (name, crate)
            type_visibility[key] = t.get("visibility", "")
    type_def_lookup = {}
    for name, _cand_crates in type_candidates.items():
        canonical = _pick_canonical_crate(type_candidates, name,
                                          impl_target_count[name])
        if canonical:
            type_def_lookup[name] = {
                "crate": canonical,
                "visibility": type_visibility.get((name, canonical), ""),
            }
    # Traits + macros use the simpler first-seen-wins for now (collisions
    # are rarer for traits, and macro names tend to be globally unique
    # within a workspace by the macro_rules convention).
    trait_def_lookup = {}
    for t in all_facts.get("traits", []):
        name = t.get("name")
        crate = t.get("crate")
        if name and crate and name not in trait_def_lookup:
            trait_def_lookup[name] = {
                "crate": crate,
                "visibility": t.get("visibility", ""),
            }
    macro_def_lookup = {}
    for m in all_facts.get("macro_defs", []):
        name = m.get("name")
        crate = m.get("crate")
        if name and crate and name not in macro_def_lookup:
            macro_def_lookup[name] = {
                "crate": crate,
                "visibility": m.get("visibility", ""),
                # 0.0.11 patch 11d: macro_exported flag from rustscan's
                # #[macro_export] attribute detection. macro_rules! by
                # itself doesn't take syntactic `pub`; #[macro_export]
                # above the declaration is the actual export mechanism.
                "macro_exported": m.get("macro_exported", False),
            }
    # 0.0.10 patch 10e: mod_def_lookup for type_usage where the outer
    # is a workspace MODULE rather than a type (tokio::sync's `mpsc` /
    # `oneshot` / `broadcast` modules are the namespaces under which
    # the channels public API lives; their factory calls are
    # `mpsc::channel()` etc.). Without mod lookup, those entries would
    # show defining_crate=None even though tokio defines those modules.
    mod_def_lookup = {}
    for m in all_facts.get("mods", []):
        name = m.get("name")
        crate = m.get("crate")
        if name and crate and name not in mod_def_lookup:
            mod_def_lookup[name] = {
                "crate": crate,
                "visibility": m.get("visibility", ""),
            }

    # 0.0.21 patch 21b: crate_name_lookup for type_usage where the outer
    # is a workspace CRATE itself (iced's `iced::application(...)` is
    # called through the crate's root namespace - there's no `mod
    # application` and no `Application` type; the public free function
    # `application(...)` lives at iced/src/lib.rs root). Without crate-
    # name lookup, those entries would show defining_crate=None even
    # though the workspace defines them. the_user 2026-06-04: 'basic
    # end-dev usage is to define your own App struct and then call
    # iced::application() with it'. Same shape applies to iced::run,
    # iced::exit, iced::daemon, and analogous crate::function entry
    # points across other workspaces.
    crate_name_lookup = {}
    crate_names_seen = set()
    for key in ("impls", "derives", "uses", "types", "traits", "fns",
                "macros", "macro_defs", "type_usages", "mods",
                "example_type_usages"):
        for f in all_facts.get(key, []):
            c = f.get("crate")
            if c:
                crate_names_seen.add(c)
    for cn in crate_names_seen:
        crate_name_lookup[cn] = {
            "crate": cn,
            # crate-as-namespace access implies pub-from-crate-root
            "visibility": "pub",
        }

    def _pattern_def(kind, pattern_inner):
        if kind in ("trait_impl", "derive"):
            return trait_def_lookup.get(pattern_inner)
        if kind == "type_usage":
            outer = pattern_inner.split("::", 1)[0]
            # Prefer type lookup; fall back to module lookup (10e); fall
            # back to crate-name lookup (21b) so iced::application etc.
            # surface as workspace-defined.
            return (type_def_lookup.get(outer)
                    or mod_def_lookup.get(outer)
                    or crate_name_lookup.get(outer))
        if kind in ("reg_macro", "attr_macro"):
            return macro_def_lookup.get(pattern_inner)
        return None

    def _pattern_match(kind, pattern_inner, fact):
        if kind == "trait_impl":
            return fact.get("trait") == pattern_inner and not fact.get("cfg_gated")
        if kind == "derive":
            return fact.get("trait") == pattern_inner
        if kind == "type_usage":
            return fact.get("name") == pattern_inner
        if kind == "reg_macro":
            return (fact.get("kind") == "macro_invocation"
                    and fact.get("name") == pattern_inner)
        if kind == "attr_macro":
            return (fact.get("kind") == "attr_macro"
                    and fact.get("name") == pattern_inner)
        return False

    metrics = {}
    sources_by_kind = {
        "trait_impl": all_facts.get("impls", []),
        "derive": all_facts.get("derives", []),
        "type_usage": all_facts.get("type_usages", []),
        "reg_macro": all_facts.get("macros", []),
        "attr_macro": all_facts.get("macros", []),
    }

    seen_patterns = set()
    for kind, source in sources_by_kind.items():
        for fact in source:
            if kind == "trait_impl":
                inner = fact.get("trait")
                if not inner or fact.get("cfg_gated"):
                    continue
            elif kind == "derive":
                inner = fact.get("trait")
                if not inner:
                    continue
            elif kind == "type_usage":
                inner = fact.get("name")
                if not inner:
                    continue
            elif kind in ("reg_macro", "attr_macro"):
                fk = fact.get("kind")
                wanted = ("macro_invocation" if kind == "reg_macro"
                          else "attr_macro")
                if fk != wanted:
                    continue
                inner = fact.get("name")
                if not inner:
                    continue
            else:
                continue
            pattern = f"{kind}:{inner}"
            seen_patterns.add((kind, inner, pattern))
    # 0.0.10 patch 10e: ALSO surface patterns that exist only in
    # example_type_usages (mpsc::channel + broadcast::channel + ...).
    # These appear nowhere in src but heavily in tests/examples; the
    # picker should rank them as architectural via the example_count
    # signal even though their src usage is zero.
    for tu in all_facts.get("example_type_usages", []):
        inner = tu.get("name")
        if inner:
            seen_patterns.add(("type_usage", inner, f"type_usage:{inner}"))

    # 0.0.10 patch 10e + 0.0.11 patch 11e + 0.0.12 patch 12d:
    # example_type_usages indexed by (file, name) with weighted
    # contribution by source directory. examples/ at 1.0x are the
    # developer-curated public-API demonstrations the_user 2026-06-03
    # framing weighted highly ('more than 3 examples exist' threshold).
    # tests/ and benches/ at 0.3x capture the the_user-confirmed
    # signal that 'tests... it couldn't be completely insignificant
    # and have tons of tests because it's a problem path' - they ARE
    # signal at lower magnitude than curated demos.
    #
    # Two output values:
    # - example_count: weighted total (gamma boost saturation input).
    # - curated_example_count: strict examples/-only count (threshold
    #   boost test, preserving the_user's '3 examples' semantics).
    test_weight = config.float_param(
        "ORIENT_TEST_WEIGHT", "picker", "example", "test_weight",
        default=0.3)
    bench_weight = config.float_param(
        "ORIENT_BENCH_WEIGHT", "picker", "example", "bench_weight",
        default=0.3)
    files_by_category_per_name = defaultdict(
        lambda: {"examples": set(), "tests": set(), "benches": set()})
    for tu in all_facts.get("example_type_usages", []):
        nm = tu.get("name")
        f = tu.get("file") or ""
        if not (nm and f):
            continue
        if "/examples/" in f or f.startswith("examples/"):
            files_by_category_per_name[nm]["examples"].add(f)
        elif "/tests/" in f or f.startswith("tests/"):
            files_by_category_per_name[nm]["tests"].add(f)
        elif "/benches/" in f or f.startswith("benches/"):
            files_by_category_per_name[nm]["benches"].add(f)
    example_files_by_name = {}
    curated_example_count_by_name = {}
    for nm, cats in files_by_category_per_name.items():
        weighted = (len(cats["examples"]) * 1.0
                    + len(cats["tests"]) * test_weight
                    + len(cats["benches"]) * bench_weight)
        example_files_by_name[nm] = weighted
        curated_example_count_by_name[nm] = len(cats["examples"])

    def _example_count_for_type_usage(name):
        # 0.0.12 patch 12d: weighted count (float). examples/ + 0.3 * tests/
        # + 0.3 * benches/ by default.
        return example_files_by_name.get(name, 0.0)

    def _curated_example_count_for_type_usage(name):
        # 0.0.12 patch 12d: strict examples/-only count for the
        # threshold semantics. the_user 2026-06-03: 'more than 3
        # examples exist' applies to curated demos, not to weighted
        # total including tests.
        return curated_example_count_by_name.get(name, 0)

    for kind, inner, pattern in seen_patterns:
        defn = _pattern_def(kind, inner)
        if defn is None:
            metrics[pattern] = {
                "defining_crate": None,
                "intra_count": 0,
                "inter_count": 0,
                "inter_ratio": 0.0,
                "is_pub": False,
                "example_count": (
                    round(_example_count_for_type_usage(inner), 2)
                    if kind == "type_usage" else 0),
                "curated_example_count": (
                    _curated_example_count_for_type_usage(inner)
                    if kind == "type_usage" else 0),
            }
            continue
        defining_crate = defn["crate"]
        is_pub = bool(defn["visibility"]) and defn["visibility"].startswith("pub")
        # 0.0.11 patch 11d: for macros, also accept #[macro_export]
        # as is_pub. Most pub macros use macro_export rather than
        # syntactic pub macro_rules!.
        if not is_pub and defn.get("macro_exported"):
            is_pub = True
        intra = 0
        inter = 0
        for fact in sources_by_kind[kind]:
            if not _pattern_match(kind, inner, fact):
                continue
            using = fact.get("crate")
            if not using:
                continue
            if using == defining_crate:
                intra += 1
            else:
                inter += 1
        total = intra + inter
        ratio = (inter / total) if total > 0 else 0.0
        metrics[pattern] = {
            "defining_crate": defining_crate,
            "intra_count": intra,
            "inter_count": inter,
            "inter_ratio": round(ratio, 3),
            "is_pub": is_pub,
            "example_count": (
                round(_example_count_for_type_usage(inner), 2)
                if kind == "type_usage" else 0),
            "curated_example_count": (
                _curated_example_count_for_type_usage(inner)
                if kind == "type_usage" else 0),
        }

    # 0.0.26: synthesize pub_type:<Name> entries from AST data.
    # For each unique identifier appearing in fn signatures + struct
    # fields + type aliases (the_user 2026-06-04: 'those two are
    # primarily where you are going to see usage'), if the identifier
    # matches a workspace-defined pub type or trait, build a
    # pattern_metrics entry with intra/inter counts attributing
    # AST hits to the using crate (the crate where the fn/struct
    # lives) vs the defining crate.
    if ast_facts and crates:
        crate_dirs = {n: c.get("dir", "") for n, c in crates.items()}
        ast_by_ident = defaultdict(list)
        for ent in ast_facts.get("fn_sig_usages", []):
            ast_by_ident[ent.get("ident", "")].append(ent)
        for ent in ast_facts.get("field_usages", []):
            ast_by_ident[ent.get("ident", "")].append(ent)
        for ent in ast_facts.get("type_alias_usages", []):
            ast_by_ident[ent.get("ident", "")].append(ent)

        for ident, hits in ast_by_ident.items():
            if not ident:
                continue
            defn = (type_def_lookup.get(ident)
                    or trait_def_lookup.get(ident))
            if not defn or not defn.get("crate"):
                continue
            vis = defn.get("visibility", "")
            if not vis.startswith("pub"):
                continue
            pattern = f"pub_type:{ident}"
            if pattern in metrics:
                continue
            defining_crate = defn["crate"]
            intra = 0
            inter = 0
            example_files = set()
            curated_files = set()
            for h in hits:
                file_path = h.get("file") or ""
                using = _resolve_crate_for_file(file_path, crate_dirs)
                if not using:
                    continue
                if using == defining_crate:
                    intra += 1
                else:
                    inter += 1
                if "/examples/" in file_path or file_path.startswith("examples/"):
                    example_files.add(file_path)
                    curated_files.add(file_path)
                elif "/tests/" in file_path or file_path.startswith("tests/"):
                    example_files.add(file_path)
                elif "/benches/" in file_path or file_path.startswith("benches/"):
                    example_files.add(file_path)
            total = intra + inter
            ratio = (inter / total) if total > 0 else 0.0
            metrics[pattern] = {
                "defining_crate": defining_crate,
                "intra_count": intra,
                "inter_count": inter,
                "inter_ratio": round(ratio, 3),
                "is_pub": True,
                "example_count": float(len(example_files)),
                "curated_example_count": len(curated_files),
            }

    # 0.0.28: synthesize method_ref:_::<inner> FAMILY entries from
    # AST method-reference data. Captures `Type::method` ExprPath
    # patterns in argument position (e.g. iced's
    # `iced::application(Clock::new, Clock::update, Clock::view)`).
    # Multi-segment only per 0.0.28 Phase 0 scoping; single-segment
    # bare refs not captured (iced examples canonically use the
    # qualified form).
    #
    # Family aggregation is the load-bearing move: each individual
    # `Type::update` ref scores intra=0, inter=1 (one call site)
    # which never reaches top-N. Aggregating across the family of
    # outers that share the inner method ("update", "view", "draw",
    # etc.) sums the architectural signal: iced's `update`-family
    # spans 50 example crates -> sum score competes with pub_type
    # entries.
    #
    # Family threshold (>= _METHOD_REF_FAMILY_MIN_OUTERS distinct
    # workspace-defined-pub outers) keeps single-outer per-Type
    # method names out (those aren't architectural protagonists).
    # Defining_crate set to most-common-outer's defining_crate so
    # the picker treats family entries as workspace-defined.
    #
    # Filters at synthesis time (NOT picker time): outer in
    # _METHOD_REF_OUTER_SKIP (Self + std trait shells); inner in
    # _METHOD_REF_INNER_SKIP (generic constructor / formatter
    # method names). Tighter than the generic-pattern picker filters
    # because method-refs grow pattern_metrics size + would inflate
    # the coverage denominator if admitted broadly.
    if ast_facts and crates:
        crate_dirs = {n: c.get("dir", "") for n, c in crates.items()}
        method_refs_by_inner = defaultdict(list)
        for ent in ast_facts.get("method_ref_usages", []):
            outer = ent.get("outer", "")
            inner = ent.get("inner", "")
            if not outer or not inner:
                continue
            if outer in _METHOD_REF_OUTER_SKIP:
                continue
            if inner in _METHOD_REF_INNER_SKIP:
                continue
            defn = (type_def_lookup.get(outer)
                    or trait_def_lookup.get(outer))
            if not defn or not defn.get("crate"):
                continue
            # 0.0.28: NO visibility filter on outer for method_ref.
            # iced example types like Clock / Editor / Tour are
            # workspace-defined but private (struct, not pub struct);
            # they're consumer types in binary crates that pass their
            # methods as fn pointers to iced::application. The
            # architectural signal is in the method ref shape, not
            # the outer's visibility.
            method_refs_by_inner[inner].append({
                "outer": outer,
                "defining_crate": defn["crate"],
                "entry": ent,
            })

        for inner, members in method_refs_by_inner.items():
            distinct_outers = {m["outer"] for m in members}
            if len(distinct_outers) < _METHOD_REF_FAMILY_MIN_OUTERS:
                continue
            pattern = f"method_ref:_::{inner}"
            if pattern in metrics:
                continue
            defining_crate = Counter(
                m["defining_crate"] for m in members
            ).most_common(1)[0][0]
            intra = 0
            inter = 0
            example_files = set()
            curated_files = set()
            for m in members:
                ent = m["entry"]
                file_path = ent.get("file") or ""
                using = _resolve_crate_for_file(file_path, crate_dirs)
                if not using:
                    continue
                if using == defining_crate:
                    intra += 1
                else:
                    inter += 1
                if "/examples/" in file_path or file_path.startswith("examples/"):
                    example_files.add(file_path)
                    curated_files.add(file_path)
                elif "/tests/" in file_path or file_path.startswith("tests/"):
                    example_files.add(file_path)
                elif "/benches/" in file_path or file_path.startswith("benches/"):
                    example_files.add(file_path)
            total = intra + inter
            ratio = (inter / total) if total > 0 else 0.0
            metrics[pattern] = {
                "defining_crate": defining_crate,
                "intra_count": intra,
                "inter_count": inter,
                "inter_ratio": round(ratio, 3),
                "is_pub": True,
                "example_count": float(len(example_files)),
                "curated_example_count": len(curated_files),
            }
    return metrics


def main():
    if tomllib is None:
        print("ERROR: tomllib unavailable (need Python 3.11+).", file=sys.stderr)
        return 2
    if len(sys.argv) < 2:
        print("usage: characterize.py <repo_root> [out_dir]", file=sys.stderr)
        return 2
    root = Path(sys.argv[1]).resolve()
    out_dir = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / ".orientation"
    out_dir.mkdir(parents=True, exist_ok=True)

    crates, workspace_roots = find_crates(root)
    if not crates:
        print(f"ERROR: no Cargo packages found under {root}", file=sys.stderr)
        return 1
    comps = components(crates)

    items_data = _run_scan_items(root, out_dir)
    global _ITEMS_BY_FILE
    _ITEMS_BY_FILE = _build_items_index(items_data)

    all_facts = {"impls": [], "traits": [], "types": [], "fns": [], "uses": [],
                 "macros": [], "derives": [], "macro_defs": [], "type_usages": [],
                 "mods": [], "example_type_usages": [],
                 "seams": Counter()}
    per_crate = {}
    free_fns_by_crate = {}
    for name, info in crates.items():
        cf = scan_crate(root, info["dir"])
        per_crate[name] = {
            "dir": info["dir"], "sloc": cf["sloc"], "deps": info["deps"],
            "n_impls": len(cf["impls"]), "n_types": len(cf["types"]),
            "n_traits": len(cf["traits"]), "n_fns": len(cf["fns"]),
            "seams": {},
        }
        free_fns_by_crate[name] = sum(1 for f in cf["fns"] if f.get("brace_depth") == 0)
        for key in ("impls", "traits", "types", "fns", "uses", "macros", "derives",
                    "macro_defs", "type_usages", "mods", "example_type_usages"):
            for rec in cf[key]:
                rec["crate"] = name
            all_facts[key] += cf[key]
    for k, v in items_data.get("seams", {}).items():
        all_facts["seams"][k] += v

    all_facts["_free_fns_by_crate"] = free_fns_by_crate
    ranked, by_kind, reg_calls = pattern_histogram(all_facts)
    del all_facts["_free_fns_by_crate"]
    sel = select_mode(ranked, by_kind, workspace_roots, len(comps))

    total_sloc = sum(c["sloc"] for c in per_crate.values()) or 1
    seam_total = sum(all_facts["seams"].values())
    seam_density = seam_total / (total_sloc / 1000.0)

    # 0.0.21 patch 21a: count .rs files under any examples/ directory
    # in the workspace. Used by emit's public-set example-weight log
    # scaling. the_user 2026-06-04: 'use number of example rs files
    # logarathmically to determine the weight applied to public
    # category'.
    example_rs_files = 0
    for rs_path in root.rglob("*.rs"):
        parts = rs_path.relative_to(root).parts
        if any(seg == "examples" for seg in parts):
            example_rs_files += 1

    # 0.0.26: AST scanner extension. Invoke rust_recon binary
    # (syn-based) to capture type-identifier occurrences in fn
    # signatures + struct fields + type aliases - the primary usage
    # sites the regex-based rustscan.py can't reach. Merged into
    # pattern_metrics as synthesized pub_type:Name entries with
    # real intra_count + inter_count from the AST hits. Also written
    # into all_facts as `ast_type_refs` so emit.py can pick them up
    # in its per_crate_counts builder.
    ast_facts = _run_ast_scan(root, out_dir, crates)
    crate_dirs = {n: c.get("dir", "") for n, c in crates.items()}
    ast_type_refs = []
    for src_key in ("fn_sig_usages", "field_usages", "type_alias_usages"):
        for ent in ast_facts.get(src_key, []):
            ident = ent.get("ident", "")
            if not ident:
                continue
            file_path = ent.get("file", "")
            using_crate = _resolve_crate_for_file(file_path, crate_dirs)
            if not using_crate:
                continue
            ast_type_refs.append({
                "name": ident,
                "file": file_path,
                "line": ent.get("line", 0),
                "crate": using_crate,
                "source": src_key,
            })
    all_facts["ast_type_refs"] = ast_type_refs

    # 0.0.28: store method_ref usages in all_facts so emit.py can
    # produce seed instances. Keyed by "<Outer>::<inner>" combined
    # name to match the pattern format produced in pattern_metrics.
    ast_method_refs = []
    for ent in ast_facts.get("method_ref_usages", []):
        outer = ent.get("outer", "")
        inner = ent.get("inner", "")
        if not outer or not inner:
            continue
        file_path = ent.get("file", "")
        using_crate = _resolve_crate_for_file(file_path, crate_dirs)
        if not using_crate:
            continue
        ast_method_refs.append({
            "name": f"{outer}::{inner}",
            "outer": outer,
            "inner": inner,
            "file": file_path,
            "line": ent.get("line", 0),
            "container": ent.get("container", ""),
            "crate": using_crate,
        })
    all_facts["ast_method_refs"] = ast_method_refs

    # 0.0.10 patches 10b + 10c + 10d: per-pattern metrics. For each
    # pattern in pattern_histogram + per-crate aggregation, compute:
    # - defining_crate: where the type / trait / macro was declared.
    # - intra_count: usages within the defining crate.
    # - inter_count: usages in other workspace crates.
    # - inter_ratio: inter_count / (intra + inter).
    # - is_pub: definition site's visibility is `pub` (10d). For
    #   trait_impl / derive patterns, looks up the trait's visibility.
    #   For type_usage patterns, looks up the outer type's visibility.
    #   For reg_macro patterns, looks up the macro_def's visibility +
    #   detects #[macro_export] attribute on it.
    pattern_metrics = _compute_pattern_metrics(all_facts, ast_facts, crates)

    # 0.0.13 patches 13d + 13e + 0.0.15 patch 15a: 4-bucket
    # use-classification at workspace level. Runs AFTER pattern_metrics
    # because the per-crate classifier weighs cross-crate is_pub usage
    # of items declared in each crate to distinguish end_with_dev_use
    # from dev_with_end_use in has-both-bin-and-lib crates.
    workspace_use_classification = _classify_workspace_use(
        crates, pattern_metrics)

    fingerprint = {
        "tool_version": "0.1.0",
        "repo_root": str(root),
        "totals": {
            "crates": len(crates), "sloc": total_sloc,
            "impls": len(all_facts["impls"]), "types": len(all_facts["types"]),
            "traits": len(all_facts["traits"]), "fns": len(all_facts["fns"]),
            "example_rs_files": example_rs_files,
        },
        "workspace_roots": workspace_roots,
        "components": comps,
        "n_components": len(comps),
        "pattern_histogram": [{"pattern": p, "count": c} for p, c in ranked[:40]],
        "pattern_by_kind": by_kind,
        "registration_macros": reg_calls,
        "seam_inventory": dict(all_facts["seams"]),
        "seam_density_per_kloc": round(seam_density, 2),
        "selection": sel,
        "pattern_metrics": pattern_metrics,
        "workspace_use_classification": workspace_use_classification,
        "thresholds": {
            "DOMINANCE_SHARE": DOMINANCE_SHARE, "COEQUAL_TOPK": COEQUAL_TOPK,
            "COEQUAL_SHARE": COEQUAL_SHARE, "AMBIGUOUS_BAND": AMBIGUOUS_BAND,
            "SEAM_DENSE_PER_KLOC": SEAM_DENSE_PER_KLOC,
            "_note": "Declared defaults, not validated constants. Override via ORIENT_* env "
                     "vars. The full histogram is reported so the choice is auditable.",
        },
        "per_crate": per_crate,
    }
    # 0.0.8 patch 8e: shape classification reads the partial fingerprint
    # + all_facts and assigns a workspace_shape. Container shape routes
    # to a routing-doc orientation in emit.py; other shapes use the
    # standard orientation with the shape label surfaced for context.
    fingerprint["workspace_shape"] = classify_workspace_shape(
        fingerprint, all_facts)

    # Fingerprint is written FIRST - the chosen mode must be auditable before any trace.
    (out_dir / "fingerprint.json").write_text(json.dumps(fingerprint, indent=2))
    all_facts["seams"] = dict(all_facts["seams"])
    (out_dir / "facts.json").write_text(json.dumps(all_facts, indent=2))

    print(f"[characterize] {len(crates)} crates, {total_sloc} SLOC, "
          f"{len(comps)} component(s), {len(workspace_roots)} workspace root(s)")
    print(f"[characterize] mode = {sel['mode']}  (histogram: {sel['histogram_mode']}, "
          f"top_share={sel['top_share']})")
    if ranked:
        print(f"[characterize] dominant pattern = {ranked[0][0]}  ({ranked[0][1]} instances)")
    for note in sel["notes"]:
        print(f"[characterize] NOTE: {note}")
    print(f"[characterize] wrote {out_dir/'fingerprint.json'} and {out_dir/'facts.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
