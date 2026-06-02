"""characterize.py — phase 1 of the orientation pipeline.

Walks a Rust workspace, builds the crate dependency graph, scans every `.rs` file for facts
(via rustscan), computes the structural fingerprint, and selects the trace mode. Writes
`fingerprint.json` (the characterization, written FIRST so the chosen mode is auditable
before any costly trace) and `facts.json` (the exhaustive fact table consumed by emit.py).

Pure stdlib. `tomllib` (3.11+) parses Cargo.toml; union-find computes disjoint components.
No clustering / community detection: region cutting (when needed) is decided by disjoint
components and, within a monolithic component, by seam density — never by graph modularity.

Thresholds are DECLARED ASSUMPTIONS, surfaced in the fingerprint and overridable via env
vars. They are defensible defaults, not constants calibrated against real repos (which this
build environment cannot run).
"""

from __future__ import annotations
import json
import os
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rustscan  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover (Python < 3.11)
    tomllib = None

# --- Declared, overridable thresholds (surfaced in the fingerprint) ---------------------
DOMINANCE_SHARE = float(os.environ.get("ORIENT_DOMINANCE_SHARE", "0.45"))
COEQUAL_TOPK = int(os.environ.get("ORIENT_COEQUAL_TOPK", "4"))
COEQUAL_SHARE = float(os.environ.get("ORIENT_COEQUAL_SHARE", "0.60"))
AMBIGUOUS_BAND = float(os.environ.get("ORIENT_AMBIGUOUS_BAND", "0.07"))
SEAM_DENSE_PER_KLOC = float(os.environ.get("ORIENT_SEAM_DENSE_PER_KLOC", "4.0"))


def find_crates(root: Path):
    """Return (crates, workspace_roots). crates: name -> {dir, deps, is_workspace_member}."""
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
            crates[name] = {
                "dir": str(cargo.parent.relative_to(root)) or ".",
                "deps": sorted(deps),
            }
    return crates, sorted(set(workspace_roots))


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


def scan_crate(root: Path, crate_dir: str):
    """Scan all .rs under a crate dir; return aggregated facts + line count."""
    agg = {"impls": [], "traits": [], "types": [], "fns": [], "uses": [],
           "macros": [], "derives": [], "macro_defs": [], "seams": Counter()}
    loc = 0
    base = root / crate_dir
    for rs in base.rglob("*.rs"):
        if "target" in rs.parts:
            continue
        try:
            src = rs.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        loc += src.count("\n") + 1
        rel = str(rs.relative_to(root))
        f = rustscan.scan_file(rel, src)
        for it in f["impls"]:
            it["file"] = rel
        for it in f["traits"] + f["types"] + f["fns"]:
            it["file"] = rel
        for it in f["macros"]:
            it["file"] = rel
        agg["impls"] += f["impls"]
        agg["traits"] += f["traits"]
        agg["types"] += f["types"]
        agg["fns"] += f["fns"]
        agg["uses"] += [dict(u, file=rel) for u in f["uses"]]
        agg["macros"] += f["macros"]
        agg["derives"] += f["derives"]
        agg["macro_defs"] += [dict(m, file=rel) for m in f["macro_defs"]]
        for k, v in f["seams"].items():
            agg["seams"][k] += v
    agg["loc"] = loc
    return agg


def pattern_histogram(all_facts):
    """Group candidate patterns by kind and specific name; return ranked list + by-kind mass.

    Candidate kinds (co-equal — the dominant pattern is decided empirically, NOT assumed to
    be `impl Trait for`):
      trait_impl:<Trait>        one entry per impl of that trait
      derive:<Trait>            one entry per derive of that trait
      attr_macro:<path>         one entry per user attribute-macro application
      reg_macro:<name>          one entry per registration-macro *argument* (call-site count,
                                expansion unverified — confirmed by the rustdoc overlay)
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

    all_facts = {"impls": [], "traits": [], "types": [], "fns": [], "uses": [],
                 "macros": [], "derives": [], "macro_defs": [], "seams": Counter()}
    per_crate = {}
    free_fns_by_crate = {}
    for name, info in crates.items():
        cf = scan_crate(root, info["dir"])
        per_crate[name] = {
            "dir": info["dir"], "loc": cf["loc"], "deps": info["deps"],
            "n_impls": len(cf["impls"]), "n_types": len(cf["types"]),
            "n_traits": len(cf["traits"]), "n_fns": len(cf["fns"]),
            "seams": dict(cf["seams"]),
        }
        free_fns_by_crate[name] = sum(1 for f in cf["fns"] if f.get("brace_depth") == 0)
        for key in ("impls", "traits", "types", "fns", "uses", "macros", "derives",
                    "macro_defs"):
            for rec in cf[key]:
                rec["crate"] = name
            all_facts[key] += cf[key]
        for k, v in cf["seams"].items():
            all_facts["seams"][k] += v

    all_facts["_free_fns_by_crate"] = free_fns_by_crate
    ranked, by_kind, reg_calls = pattern_histogram(all_facts)
    del all_facts["_free_fns_by_crate"]
    sel = select_mode(ranked, by_kind, workspace_roots, len(comps))

    total_loc = sum(c["loc"] for c in per_crate.values()) or 1
    seam_total = sum(all_facts["seams"].values())
    seam_density = seam_total / (total_loc / 1000.0)

    fingerprint = {
        "tool_version": "0.1.0",
        "repo_root": str(root),
        "totals": {
            "crates": len(crates), "loc": total_loc,
            "impls": len(all_facts["impls"]), "types": len(all_facts["types"]),
            "traits": len(all_facts["traits"]), "fns": len(all_facts["fns"]),
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
        "thresholds": {
            "DOMINANCE_SHARE": DOMINANCE_SHARE, "COEQUAL_TOPK": COEQUAL_TOPK,
            "COEQUAL_SHARE": COEQUAL_SHARE, "AMBIGUOUS_BAND": AMBIGUOUS_BAND,
            "SEAM_DENSE_PER_KLOC": SEAM_DENSE_PER_KLOC,
            "_note": "Declared defaults, not validated constants. Override via ORIENT_* env "
                     "vars. The full histogram is reported so the choice is auditable.",
        },
        "per_crate": per_crate,
    }

    # Fingerprint is written FIRST — the chosen mode must be auditable before any trace.
    (out_dir / "fingerprint.json").write_text(json.dumps(fingerprint, indent=2))
    all_facts["seams"] = dict(all_facts["seams"])
    (out_dir / "facts.json").write_text(json.dumps(all_facts, indent=2))

    print(f"[characterize] {len(crates)} crates, {total_loc} LoC, "
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
