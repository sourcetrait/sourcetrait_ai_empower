"""normalize_for_compare.py - canonicalize a know_rust json output for
semantic-equivalence comparison.

The python characterize.py emitted dicts whose key order depended on
the Python set / dict hash randomization, so the same logical content
serializes to different bytes across runs (PYTHONHASHSEED randomizes
per-process). The Rust port uses IndexMap with deterministic
insertion order. To verify the Rust port produces SEMANTICALLY the
same content as a Python baseline, this script canonicalizes both
outputs to a stable form (alphabetical keys + list-entry sort) so a
byte-diff after normalization equals a semantic-content equality.

Usage:
  python3 normalize_for_compare.py <input.json> > <output.json>
  diff <(normalize baseline.json) <(normalize new.json)

This is a temporary bridge utility. It can be deleted when the
python orientation pipeline is fully retired and the baseline is
re-generated from the Rust port (which produces deterministic
output by construction).
"""
from __future__ import annotations
import json
import sys


def canon(value):
    """Recursively canonicalize a JSON-shaped value: sort dict keys
    alphabetically and sort list entries by their canonical-form JSON
    string.

    Why list sort: facts.json arrays (impls, derives, type_usages,
    etc.) come from the per-crate aggregation loop whose order depends
    on filesystem walk order (rglob vs walkdir). Sorting entries by
    their canonical-form string makes the comparison order-independent
    while still detecting any content difference.
    """
    if isinstance(value, dict):
        return {k: canon(value[k]) for k in sorted(value.keys())}
    if isinstance(value, list):
        canonicalized = [canon(v) for v in value]
        # Sort by the JSON serialization of each entry; stable for
        # mixed-shape lists.
        return sorted(canonicalized, key=lambda x: json.dumps(x, sort_keys=True))
    return value


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: normalize_for_compare.py <input.json>", file=sys.stderr)
        return 2
    with open(argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
    canonical = canon(data)
    print(json.dumps(canonical, indent=2, sort_keys=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
