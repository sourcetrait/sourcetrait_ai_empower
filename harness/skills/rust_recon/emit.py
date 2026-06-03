"""emit.py — final phase. Reads fingerprint.json + facts.json and writes the two-file
artifact:

  reference.md   exhaustive, span-anchored, grep-into. Fully mechanical.
  orientation.md read-first, map-first. The mechanical skeleton (provenance, crate/region
                 map, core type vocabulary, seam-spine, dominant-pattern identification,
                 a candidate instance, detected-seam UNRESOLVED guardrails) plus clearly
                 marked [AGENT ...] slots that Claude Code fills BY READING SOURCE at the
                 spans provided — the judgment work (data-flow narrative, the worked slice,
                 the why-axis from doc-comments) that no static tool can fabricate.

The division is deliberate: the tool produces everything that is fabrication-proof (counts,
spans, graph, detected boundaries); the agent authors everything requiring judgment, always
anchored to tool-provided spans so it cannot drift into fiction. Generation cost is ignored;
the reference is exhaustive.
"""

from __future__ import annotations
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path


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
                L.append(f"- `{t['name']}` — {sp(t)}{cfg}")
        if c["types"]:
            L.append("### types")
            for t in sorted(c["types"], key=lambda x: x["name"]):
                cfg = "  *(cfg-gated)*" if t.get("cfg_gated") else ""
                L.append(f"- `{t['kind']} {t['name']}` — {sp(t)}{cfg}")
        if c["impls"]:
            L.append("### impls")
            for i in sorted(c["impls"], key=lambda x: (str(x.get("trait")), str(x.get("type")))):
                tr = f"`{i['trait']}` for " if i.get("trait") else "(inherent) "
                cfg = "  *(cfg-gated)*" if i.get("cfg_gated") else ""
                L.append(f"- impl {tr}`{i.get('type')}` — {sp(i)}{cfg}")
        if c["fns"]:
            L.append("### free functions")
            for f in sorted(c["fns"], key=lambda x: x["name"]):
                L.append(f"- `fn {f['name']}` — {sp(f)}")
        if c["macros"]:
            L.append("### macro applications *(expansion unverified without rustdoc overlay)*")
            for m in sorted(c["macros"], key=lambda x: x.get("line", 0)):
                if m["kind"] == "macro_invocation":
                    args = ", ".join(m.get("arg_idents", [])[:12])
                    L.append(f"- `{m['name']}!(...)` args=[{args}] — {sp(m)}")
                else:
                    L.append(f"- `#[{m['name']}]` — {sp(m)}")
        if c["reexports"]:
            L.append("### re-exports *(rustdoc resolves the target; span may be null)*")
            for u in c["reexports"]:
                L.append(f"- `{u['path']}` — {sp(u)}")
        L.append("")
    out.write_text("\n".join(L))


def core_vocabulary(fp: dict, facts: dict):
    """Heuristic: the core types live in the most-depended-on crate. Prefer in-workspace
    crates (those that appear in fp["per_crate"] keys) over external infra crates -- the §2
    vocabulary should be the domain language other crates in the workspace speak in, not a
    shared error-helper or utility crate from crates.io. Falls back to the global most-
    depended-on pick only when no in-workspace crate has any dependents at all (unusual)."""
    dep_count = defaultdict(int)
    for c in fp["per_crate"].values():
        for d in c["deps"]:
            dep_count[d] += 1
    in_workspace = {k: v for k, v in dep_count.items() if k in fp["per_crate"]}
    pick_pool = in_workspace if in_workspace else dep_count
    core = max(pick_pool, key=pick_pool.get) if pick_pool else None
    # Filter types/traits to src/ only -- the 0.0.2 #5 sweep partition for §3 seam sites,
    # now extended to §2 vocab so test-file types (ratatui/tests/*.rs, tokio/tests/*.rs)
    # do not pollute the listed core vocabulary.
    types = [t for t in facts["types"]
             if t.get("crate") == core and _is_src_file(t.get("file", ""))]
    traits = [t for t in facts["traits"]
              if t.get("crate") == core and _is_src_file(t.get("file", ""))]
    return core, types, traits


def candidate_instance(fp: dict, facts: dict):
    """Pick one instance of the dominant pattern as the worked-slice seed."""
    if not fp["pattern_histogram"]:
        return None
    dom = fp["pattern_histogram"][0]["pattern"]
    kind, _, name = dom.partition(":")
    if kind == "trait_impl":
        inst = [i for i in facts["impls"] if i.get("trait") == name and not i.get("cfg_gated")]
        inst.sort(key=lambda x: str(x.get("type")))
        return {"kind": kind, "pattern": dom,
                "instance": inst[0] if inst else None,
                "all_spans": [sp(i) for i in inst[:200]]}
    if kind == "derive":
        inst = [d for d in facts["derives"] if d.get("trait") == name]
        return {"kind": kind, "pattern": dom, "instance": inst[0] if inst else None,
                "all_spans": [f"{d.get('file','?')}:{d['line']}" for d in inst[:200]]}
    if kind == "reg_macro":
        inst = [m for m in facts["macros"]
                if m["kind"] == "macro_invocation" and m["name"] == name]
        return {"kind": kind, "pattern": dom, "instance": inst[0] if inst else None,
                "all_spans": [sp(m) for m in inst[:200]]}
    return {"kind": kind, "pattern": dom, "instance": None, "all_spans": []}


def _is_src_file(file_path: str) -> bool:
    """True if the file lives under src/ rather than tests/ / benches/ / examples/.
    Architectural seams live in src/; test-harness seams crowd the seam list otherwise --
    e.g. sourcetrait_empower's process_spawn signal was dominated by tests/*.rs Host
    harnesses before this filter landed. Handles two layouts: per-crate tests under
    `crates/<X>/tests/...` (slash-segment form) and workspace-top-level tests under
    `tests/...` (path-prefix form, as nushell uses for integration tests). Used by
    detected_seams to keep §3 focused on the architectural signal."""
    excluded = ("tests/", "benches/", "examples/")
    if any(file_path.startswith(p) for p in excluded):
        return False
    return all(f"/{p}" not in file_path for p in excluded)


def detected_seams(fp: dict, facts: dict):
    """Seams the static trace cannot cross -- seeds for the UNRESOLVED guardrail list.
    Spawn-site filtering is src/-only so test-harness sites do not crowd the architectural
    signal (see _is_src_file)."""
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
        seeds.append((f"macro-mediated registration ({macs})",
                      "Items are registered by a macro; the call-site argument list is "
                      "captured but the EXPANSION is invisible to the scanner. Counts are "
                      "unverified without the rustdoc overlay. Confirm the generated items "
                      "in source or via overlay before relying on the registry.", []))
    if inv.get("extern") or inv.get("syscall_libc"):
        seeds.append(("FFI / syscall boundary",
                      "`extern`/`libc` crosses into non-Rust or the kernel; static tracing "
                      "stops at the boundary. Verify the foreign contract before authoring.",
                      []))
    return seeds


def emit_orientation(root: Path, fp: dict, facts: dict, out: Path):
    sel = fp["selection"]
    core, core_types, core_traits = core_vocabulary(fp, facts)
    cand = candidate_instance(fp, facts)
    seams = detected_seams(fp, facts)

    L = ["# Orientation", "",
         "Read-first. This is the map; the source is the territory and `reference.md` is the",
         "exhaustive index. Map-first ordering: skeleton (crate map, core vocabulary, seams,",
         "flow) then the worked slice (the authoring template), then guardrails, then the",
         "authoring guide. Every claim is a span you can open. Sections marked **[AGENT]** are",
         "filled by reading source at the cited spans — never from guesswork.", "",
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
    if fp["n_components"] > 1 or len(fp["workspace_roots"]) > 1:
        L.append(f"**Regional** — {fp['n_components']} disjoint component(s), "
                 f"{len(fp['workspace_roots'])} workspace root(s). Each component is a region; "
                 f"the seam-spine (§3) is the join. **[AGENT]** name each region's role and "
                 f"the named seams connecting it to the others; if two regions share no traced "
                 f"data path, record that as an UNRESOLVED rather than inventing a link.")
        L.append("")
        for idx, comp in enumerate(fp["components"], 1):
            L.append(f"- region {idx}: {', '.join(comp)}")
    else:
        L.append("Single connected component. Crates and their internal dependencies:")
    L.append("")
    for name in sorted(fp["per_crate"]):
        c = fp["per_crate"][name]
        ideps = [d for d in c["deps"] if d in fp["per_crate"]]
        dep_str = f" → depends on: {', '.join(ideps)}" if ideps else ""
        L.append(f"- **{name}** ({c['dir']}/, {c['loc']} LoC, {c['n_impls']} impls, "
                 f"{c['n_types']} types){dep_str}")
    L.append("")
    L.append("**[AGENT]** In 2–4 sentences each (what / why / where), describe the role of the "
             "core crates. Populate *why* only from crate-level doc-comments / README; where "
             "absent, write `why: unverified`.")
    L.append("")

    # 2. core type vocabulary
    L += ["## 2. Core type vocabulary", "",
          f"Most-depended-on crate: **{core}** — its public types are the vocabulary other "
          f"crates speak in. Confirm and describe each (what / where load-bearing; why from "
          f"doc-comments else unverified):", ""]
    for t in sorted(core_traits, key=lambda x: x["name"])[:40]:
        doc = f" — doc: {t['doc'][:120]}" if t.get("doc") else "  *(why: unverified — no doc)*"
        L.append(f"- trait `{t['name']}` — {sp(t)}{doc}")
    for t in sorted(core_types, key=lambda x: x["name"])[:40]:
        doc = f" — doc: {t['doc'][:120]}" if t.get("doc") else "  *(why: unverified — no doc)*"
        L.append(f"- `{t['kind']} {t['name']}` — {sp(t)}{doc}")
    L.append("")

    # 3. seam-spine
    L += ["## 3. Seam-spine", "",
          "Where the workspace stops being one connected thing. These are where architect-",
          "level features add or cross boundaries, and where the static trace stops honestly.",
          ""]
    if seams:
        for title, desc, sites in seams:
            L.append(f"- **{title}** — {desc}")
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
          "**[AGENT]** Trace how the core data type (from §2) moves from entry to result "
          "through the core crates. 1–2 short paragraphs, each sentence anchored to a span "
          "from reference.md. Stop at any seam from §3 with an explicit UNRESOLVED.", ""]

    # 5. worked slice (the protagonist; seeded)
    L += ["## 5. Worked slice — the authoring template", ""]
    if cand and cand.get("instance"):
        dom = cand["pattern"]
        inst = cand["instance"]
        L.append(f"Dominant pattern: **`{dom}`** "
                 f"({fp['pattern_histogram'][0]['count']} instances; "
                 f"this is the kind you will most often author).")
        if cand["kind"] == "trait_impl":
            L.append(f"Seed instance: `impl {dom.split(':')[1]} for {inst.get('type')}` "
                     f"— {sp(inst)}.")
        else:
            L.append(f"Seed instance — {sp(inst)}.")
        L.append("")
        L.append("**[AGENT]** Trace THIS ONE instance across every crate boundary it touches, "
                 "as the executable template for authoring the next one:")
        L += ["- **what** it does: inputs / outputs / state + environment changes "
              "(load-bearing — read the impl body in source).",
              "- **where** it plugs in: how it is registered and invoked (follow the "
              "registration path; if it goes through a macro, that is a guardrail — see §6).",
              "- **why** it is shaped this way: from doc-comments only, else `why: unverified`.",
              "- stop honestly at each seam (§3) with `UNRESOLVED: what you looked for, what "
              "you ran`. A stop is a success — it marks a real boundary for the next author."]
        L.append("")
        L.append("Every other instance of this pattern (open any to compare): "
                 + ", ".join(f"`{s}`" for s in cand["all_spans"][:15])
                 + (" …" if len(cand["all_spans"]) > 15 else ""))
    else:
        L.append("**[AGENT]** No single dominant instance was isolated automatically "
                 f"(mode: {sel['mode']}). Pick the largest pattern from the histogram below "
                 "and trace one instance of it as the template.")
    L.append("")

    # 6. UNRESOLVED guardrails
    L += ["## 6. UNRESOLVED guardrails", "",
          "Do not author *across* these without verifying in source first — a guessed bridge "
          "compiles but is wrong. Seeded from detected boundaries; **[AGENT]** add any trace "
          "stop you hit.", ""]
    if fp.get("registration_macros"):
        for mac, n in fp["registration_macros"].items():
            L.append(f"- **`{mac}!` registration** — expansion invisible to the scanner; "
                     f"call-site arg counts are unverified. Confirm generated items in source "
                     f"or via the rustdoc overlay before relying on the registry.")
    for title, desc, _ in seams:
        L.append(f"- **{title}** — {desc}")
    if not fp.get("registration_macros") and not seams:
        L.append("- None seeded. Record trace stops here as you hit them.")
    L.append("")

    # 7. pattern-authoring guide
    L += ["## 7. Pattern-authoring guide", "",
          "**[AGENT]** From the trait/struct definitions in §2 and the worked slice in §5, "
          "write the minimal checklist to author a NEW instance of the dominant pattern: which "
          "trait to implement, which methods are required (read the trait def in source), how "
          "to register it (the path from §5), and which seams (§6) a new instance must "
          "respect. Anchor each step to a span.", "",
          "## Appendix: full pattern histogram", ""]
    for row in fp["pattern_histogram"][:25]:
        L.append(f"- `{row['pattern']}` — {row['count']}")
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
