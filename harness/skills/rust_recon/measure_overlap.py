"""measure_overlap.py - 0.0.18 + 0.0.22: strict-overlap measurement
harness. Reads a structured manual-ground-truth JSON (the_user-validated
architectural-protagonist list per target) + parses S5.1..5.5 pick
entries from orientation.md per target in a baseline directory, then
reports per-target overlap + missing + aggregate.

Usage:
  python3 measure_overlap.py <baseline_dir> <ground_truth_json>

  baseline_dir: directory containing per-target subdirs, each with an
                orientation.md (e.g. notes/rust_recon/historical/recon/0.0.22/).
  ground_truth_json: path to manual_ground_truth.json.

Output: human-readable scoreboard to stdout. Exit 0 always (this is a
measurement tool, not a gate).

Matching semantics: each ground-truth entry has aliases (case-sensitive
substrings). An entry is matched if any alias appears as a substring
in any pick across S5.1..5.5 (architecture, public, inter-crate,
intra-crate, inner-crate). The picker output items are like
'derive:Component' or 'type_usage:World::new'; the substring match
runs against the full kind:name string.
"""

from __future__ import annotations
import json
import re
import sys
from pathlib import Path


_PICK_LINE_RE = re.compile(r"^-\s+`([^`]+)`")


def parse_picks(orientation_text: str) -> dict[str, list[str]]:
    """Parse S5.1..5.5 pick lists from an orientation.md.

    Returns a dict:
      {'architecture': [...], 'public': [...], 'inter_crate': [...],
       'intra': [...], 'inner_crate': [...]}
    where each list contains kind:name strings from the bullet lines.

    The workspace-wide sets (architecture / public / inter-crate) are
    single lists; the per-crate sets (intra / inner-crate) are the
    union across all per-crate sub-lists. The combined picks set is
    the union across all five.
    """
    lines = orientation_text.splitlines()
    sections = {"architecture": [], "public": [], "inter_crate": [],
                "intra": [], "inner_crate": []}
    current_section: str | None = None
    in_s5 = False
    for line in lines:
        stripped = line.rstrip()
        if stripped.startswith("## 5."):
            in_s5 = True
        if in_s5 and stripped.startswith("## ") and not stripped.startswith("## 5."):
            in_s5 = False
            current_section = None
            continue
        if not in_s5:
            continue
        if stripped.startswith("### 5.1"):
            current_section = "architecture"
            continue
        if stripped.startswith("### 5.2"):
            current_section = "public"
            continue
        if stripped.startswith("### 5.3"):
            current_section = "inter_crate"
            continue
        if stripped.startswith("### 5.4"):
            current_section = "intra"
            continue
        if stripped.startswith("### 5.5"):
            current_section = "inner_crate"
            continue
        # End of significance lists when we hit the [AGENT] header.
        if stripped.startswith("**[AGENT]"):
            current_section = None
            continue
        if current_section is None:
            continue
        match = _PICK_LINE_RE.match(stripped)
        if match:
            sections[current_section].append(match.group(1))
    return sections


def match_ground_truth(picks: dict[str, list[str]],
                       ground_truth: list[dict]) -> list[dict]:
    """Per ground-truth entry, find which picks match.

    Returns a list of dicts: [{label, aliases, matched_picks (list),
    matched (bool)}].
    """
    all_picks = (picks["architecture"] + picks["public"]
                 + picks["inter_crate"] + picks["intra"]
                 + picks["inner_crate"])
    results = []
    for gt in ground_truth:
        aliases = gt.get("aliases", [])
        matched_picks = []
        for pick in all_picks:
            for alias in aliases:
                if alias in pick:
                    matched_picks.append(pick)
                    break
        # Dedup matched_picks.
        seen = set()
        deduped = []
        for p in matched_picks:
            if p not in seen:
                seen.add(p)
                deduped.append(p)
        results.append({
            "label": gt.get("label"),
            "aliases": aliases,
            "matched_picks": deduped,
            "matched": len(deduped) > 0,
        })
    return results


def score_target(orientation_path: Path,
                 ground_truth: list[dict]) -> dict:
    """Score a single target: load orientation.md, parse picks,
    match against ground_truth, return summary dict."""
    text = orientation_path.read_text(encoding="utf-8", errors="replace")
    picks = parse_picks(text)
    matches = match_ground_truth(picks, ground_truth)
    n_total = len(ground_truth)
    n_matched = sum(1 for m in matches if m["matched"])
    pct = (100.0 * n_matched / n_total) if n_total > 0 else 0.0
    return {
        "n_architecture_picks": len(picks["architecture"]),
        "n_public_picks": len(picks["public"]),
        "n_inter_crate_picks": len(picks["inter_crate"]),
        "n_intra_picks": len(picks["intra"]),
        "n_inner_crate_picks": len(picks["inner_crate"]),
        "n_total_picks_union": len(set(
            picks["architecture"] + picks["public"]
            + picks["inter_crate"] + picks["intra"]
            + picks["inner_crate"])),
        "n_ground_truth": n_total,
        "n_matched": n_matched,
        "overlap_pct": pct,
        "matches": matches,
    }


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: measure_overlap.py <baseline_dir> <ground_truth_json>",
              file=sys.stderr)
        return 2
    baseline_dir = Path(sys.argv[1]).resolve()
    gt_path = Path(sys.argv[2]).resolve()
    if not baseline_dir.is_dir():
        print(f"ERROR: baseline dir {baseline_dir} not a directory",
              file=sys.stderr)
        return 1
    if not gt_path.is_file():
        print(f"ERROR: ground truth {gt_path} not found", file=sys.stderr)
        return 1

    gt_data = json.loads(gt_path.read_text(encoding="utf-8"))
    targets = gt_data.get("targets", {})

    print(f"== overlap scoreboard ==")
    print(f"baseline: {baseline_dir}")
    print(f"ground_truth: {gt_path}")
    print()
    print(f"{'target':<22} {'overlap':>9}  {'matched':>10}  {'picks (union)':>15}")
    print("-" * 65)

    per_target = []
    measurable_pct = []
    for tname, tinfo in sorted(targets.items()):
        target_dir = baseline_dir / tname
        orient_path = target_dir / "orientation.md"
        if not orient_path.is_file():
            print(f"{tname:<22} {'(no orientation.md)':>40}")
            continue
        ground_truth = tinfo.get("ground_truth", [])
        if not ground_truth:
            skip = tinfo.get("skip_reason", "no ground truth list")
            print(f"{tname:<22} {'(skip)':>9}              -  {skip}")
            continue
        result = score_target(orient_path, ground_truth)
        print(f"{tname:<22} {result['overlap_pct']:>8.1f}% "
              f"{result['n_matched']:>4}/{result['n_ground_truth']:<4}  "
              f"{result['n_total_picks_union']:>15}")
        per_target.append({"target": tname, **result})
        measurable_pct.append(result["overlap_pct"])

    print("-" * 65)
    if measurable_pct:
        avg = sum(measurable_pct) / len(measurable_pct)
        print(f"{'AVERAGE':<22} {avg:>8.1f}%   "
              f"({len(measurable_pct)} measurable targets)")
    print()

    # Detail: per-target matched + missing.
    print("== per-target detail ==")
    for entry in per_target:
        print(f"\n## {entry['target']} - {entry['overlap_pct']:.1f}% "
              f"({entry['n_matched']}/{entry['n_ground_truth']})")
        for m in entry["matches"]:
            sym = "[+]" if m["matched"] else "[-]"
            matched_str = (
                f" via {m['matched_picks'][:3]}"
                + (f" (+{len(m['matched_picks']) - 3} more)"
                   if len(m['matched_picks']) > 3 else "")
                if m["matched"] else " MISSING"
            )
            print(f"  {sym} {m['label']}{matched_str}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
