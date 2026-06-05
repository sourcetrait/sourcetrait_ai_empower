---
name: know_rust 
description: Generate a durable, span-anchored architectural orientation for a large or unfamiliar Rust workspace. Use when you need to understand how a Rust codebase is structured before authoring non-trivial features - when asked to "orient", "map", "get up to speed on", "understand the architecture of", or "find the main pattern in" a Rust repo, monorepo, or Cargo workspace, especially codebases where reading top-to-bottom is infeasible. Also use to refresh that understanding after a clean session or context compaction. Produces two files (orientation.md + reference.md) on disk, not a chat answer.
---

# Repo Orientation

## What this produces and who reads it

This skill writes a two-file orientation artifact into a Rust workspace, anchored to the
current commit:

- **`.orientation/orientation.md`** - read-first. The map: crate/region structure, core type
  vocabulary, the seam-spine, a data-flow narrative, and ONE worked vertical slice of the
  workspace's dominant repeated pattern, traced across crate boundaries. It is small and
  relational; it never approaches source size.
- **`.orientation/reference.md`** - exhaustive, span-anchored index. Grep into it; never read
  it linearly.

The intended reader is a future agent (often you, after a clean session or a compaction) that
needs to author **architect-level features into this repo** without re-deriving the
architecture from scratch. The orientation is the executable map back into the source. So
optimize every section for *authoring*: what a thing does and where it plugs in are
load-bearing; why it is shaped that way is supporting context, valuable only when it is true.

The method this skill embodies is the property it must also exhibit: **many instances of few
kinds.** Find the workspace's dominant repeated pattern empirically, trace one instance of it
as a worked slice, and let that slice be the template for authoring the next instance.

## The two-part division of labor (read this before running)

This skill is deliberately split between deterministic tooling and your judgment:

- **The `know_rust` binary does the fabrication-proof work** - counting, span extraction, the
  crate graph, disjoint-component detection, the pattern histogram, seam detection, mode
  selection. These are facts; the tool cannot make them up, and you should not second-guess
  them without opening source.
- **You do the judgment work** - the trace, the data-flow narrative, the why-axis - but
  *always anchored to the spans the tool gives you*. Reading source at a provided `file:line`
  is how you stay honest. Writing a claim you did not open in source is how the artifact rots.

The emitted `orientation.md` is a skeleton with sections marked **`[AGENT]`**. Those are
yours to fill. Everything else is already grounded.

## Workflow

### Step 1 - Characterize (writes the fingerprint first)

```
know_rust characterize <repo_root> <repo_root>/.orientation
```

This walks the workspace, scans every `.rs` file, builds the crate dependency graph, and
writes `fingerprint.json` (the structural characterization + chosen trace mode) **before**
anything expensive, plus `facts.json` (the exhaustive fact table). It prints a summary.

The fingerprint is written first on purpose: the mode decision is auditable before you invest
in a trace. Read it.

### Step 2 - Review the fingerprint and confirm the mode

Open `fingerprint.json`. Look at:

- **`pattern_histogram`** - the empirical ranking of candidate patterns. The top entry is the
  dominant pattern you will trace. Co-equal kinds (trait-impl, derive, attribute-macro,
  registration-macro, function-table) compete here; the winner is *not* assumed to be
  `impl Trait for`. If two patterns are close, both matter.
- **`selection.mode`** - `single_dominant`, `co_equal_few`, `no_dominant`, or `regional`.
- **`selection.notes`** - method-selection honesty. If the mode is ambiguous (near a
  threshold) or was forced to `regional` by structure, the note says so. **Trust the note.**
  If it reports a runner-up mode, hold both in mind when you trace.
- **`seam_inventory`** and **`registration_macros`** - the boundaries the static scan found.

If a threshold looks wrong for this repo, re-run Step 1 with a custom calibration TOML
(e.g. `know_rust -c <path/to/custom.toml> characterize ...`); the embedded default is
`crates/know_rust/assets/calibration.toml`. The thresholds are declared defaults, not
validated constants - see the `thresholds` block in the fingerprint.

### Step 3 - Emit the skeleton

```
know_rust emit <repo_root> <repo_root>/.orientation
```

This writes `reference.md` (complete) and `orientation.md` (skeleton with `[AGENT]` slots,
pre-seeded with the dominant pattern, a candidate instance and all its sibling spans, the
detected seams, and the UNRESOLVED guardrails). It prints the orientation/reference line
ratio as a sanity check that the map stayed small.

### Step 4 - Fill the `[AGENT]` sections by reading source

This is the core of the work. Open `orientation.md` and fill each `[AGENT]` slot **by opening
the cited spans in source** - never from memory or inference. The slots, in authoring order:

1. **S1 crate/region roles** - what each core crate is for, where it sits.
2. **S2 core vocabulary** - confirm and describe the core types other crates speak in.
3. **S3 seam-spine** - trace the dominant pattern UP TO each seam and stop.
4. **S4 data-flow narrative** - how the core data type moves entry -> result.
5. **S5 worked slice** - the protagonist. Trace the one seeded instance across every boundary
   it touches. This becomes the template for authoring new instances.
6. **S7 pattern-authoring guide** - the minimal checklist to add a new instance of the
   dominant pattern.

Honor three contracts while filling them (full guidance in `references/method.md`):

- **Three axes, weighted.** Each item is *what* it does, *where* it plugs in, *why* it is
  shaped that way - 1-2 sentences. *What* and *where* are load-bearing because the reader
  writes code against them. *Why* is supporting.
- **Anti-fabrication on the why-axis.** The reader will write code based on this. A wrong
  *why* causes a real defect. State *why* only from doc-comments or unambiguous code; where
  you cannot verify it, write `why: unverified`. The skeleton already marks undocumented
  items this way - leave the mark rather than inventing a rationale.
- **UNRESOLVED is a guardrail, not a failure.** When a trace stops at a macro expansion, an
  IPC/process boundary, an FFI/syscall edge, or anything you cannot follow in source, write
  `UNRESOLVED: <what you looked for>, <what you ran>`. A recorded stop tells the next author
  "do not write across this seam unverified" - which is exactly what protects them. Never
  narrate over a gap.

### Step 5 (optional) - Rustdoc semantic overlay

If a nightly toolchain is present and the project builds, enrich the artifact with
post-macro-expansion ground truth (real macro-generated item counts, re-export targets,
floor/rustdoc disagreements that confirm seams):

```
know_rust rustdoc-overlay <repo_root> <repo_root>/.orientation [package]
```

If no nightly cargo is available it writes a `status: absent` banner and the floor-only
artifact stands unchanged - it is still correct and honest, just without the overlay. After
running it, fold any resolved counts and disagreements into S5/S6 and flip the provenance
`rustdoc_overlay_present` flag by re-running Step 3 (the emitter reads the overlay's presence).

### Step 6 - Finalize

Re-read `orientation.md` end to end with fresh eyes. Every claim should be a span you could
open, every gap should be marked, and the map should be readable in one sitting. The
provenance header (commit, rustc, tool version, overlay flag) makes the snapshot
self-describing - staleness is acceptable because the artifact says exactly what commit it
describes; regenerate against a new commit by re-running from Step 1.

## Degradation

The pure-stdlib floor (Steps 1-4) produces a complete, honest artifact with no external
dependencies and no network. tree-sitter (if a wheel is importable) and the rustdoc overlay
(if a nightly toolchain exists) only make it sharper. Never block on them.

## When NOT to use this

For a quick question about one file or function, just read the file. This skill is for
building a durable map of a workspace too large to hold in your head - its cost is justified
only when that map will be reused for authoring.
