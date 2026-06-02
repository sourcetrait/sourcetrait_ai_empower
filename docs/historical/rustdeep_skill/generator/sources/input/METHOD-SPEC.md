# Method Spec — Procreative Codebase Orientation

**Status:** repo-agnostic. This is what the skill *encodes*. No repository-specific
fact may appear here; concrete repos live only in `VALIDATION-FIXTURES.md`.

**Self-containment requirement:** a fresh Claude instance with no conversation history
must be able to build the skill from this file plus `BUILD-SCAFFOLD.md`. Non-obvious
decisions therefore carry their rationale inline — the reasoning has to travel, because
the builder cannot re-derive a conversation it never saw.

---

## 1. Purpose and the procreative-rebuild property

The skill produces a durable on-disk orientation artifact for a large Rust workspace: files
that a max-effort **Claude Code** instance loads after a clean session or a context
compaction, becoming immediately capable of **authoring architect-level features and fixes
into the repository** across a multi-session campaign. It restores capability that a wiped
or compacted context destroyed, without re-reading the whole tree.

The deeper property: a fresh instance regenerates the artifact against a new commit with
**no conversation history**. The artifact and its generator stand alone. A per-commit
snapshot is a feature, not staleness; add no staleness mitigation.

**Out of scope:** campaign/progress state (what the effort has done and plans next) is
handled by a separate harness. This skill produces the **architectural capability substrate
only** — slow-changing, regenerated when the code's structure drifts.

## 2. Optimization target — everything bends to this

**Primary consumer: a max-effort Claude Code instance rehydrating after a clean session or
compaction, in order to author architect-level features and fixes into the repository.** The
reader is an agentic coding instance at peak competence — not a human, not a chat instance.
It will *act* on the artifact by writing code, so the artifact's errors become **defects in
the repository**. That raises every honesty constraint below to defect-prevention.

This single choice shapes the output:

- The worked vertical slice is the **protagonist**: the executable template the rehydrated
  instance copies and adapts to author the next feature. It is traced, not summarized (§4).
- **What and where are the load-bearing axes** (§6). To extend a codebase you need what a
  building block does (I/O, state, environment changes) and where it plugs into the flow.
  *Why* is supporting — still recorded, still fabrication-guarded, but not primary.
- **Core type vocabulary and the seam-spine are primary alongside the slice.** Architect-
  level features extend the skeleton — new core types, new or crossed seams — not merely add
  a leaf instance. A rehydrated author needs the vocabulary every crate speaks and the
  boundaries it must respect or cross, operationally, before it can land a cross-cutting
  change.
- **UNRESOLVED is an authoring guardrail** (§7): each gap marks a seam the next instance must
  verify against source before writing, never guess a bridge through. A confabulated seam
  produces code that compiles but fails at a process/IPC/syscall boundary.

The orientation is **map-first** — not for approachability, but because you cannot author a
cross-cutting feature without the relational skeleton and seams in hand; the slice then
threads one concrete instance through that skeleton as the template.

The bar is **authoring-actionable density** at maximum competence: no pedagogy, no narrative
for its own sake, no restatement of what a span already routes to. Every line reduces the
work of landing a correct change.

Secondary uses — answering arbitrary architectural questions, and locating/tracing
runtime and error paths for fixes — fall out of the reference index as byproducts; their
depth routes to the reference, never into the read-first orientation, which the read-first
constraint caps.

## 3. Core thesis (locked)

Large workspaces are **many instances of few kinds**. You understand one by (a) finding the
dominant repeated pattern(s) empirically, (b) tracing **one** instance of the dominant
pattern as a worked vertical slice across the boundaries it crosses, and (c) mapping the
seams where that slice — or the workspace — stops being one connected thing. Tracing one
exemplar of a kind gives the author a verified template for every instance of that kind;
additional instances of the same kind are length, not depth.

## 4. Pipeline

`characterize → select → partition (if regional) → trace → emit`

Every branch is chosen by **measurement**, never hardcoded per repo.

### 4.1 Characterize  *(write output to disk first)*

Compute a small structural fingerprint and persist it before any tracing. It is the
cheapest thing to get human eyes on, it is the *explanation* of why the orientation is
shaped as it is, and a rebuild that re-derives it can detect "the repo changed shape" as a
first-class signal. Signals (all stdlib-computable — see `BUILD-SCAFFOLD.md`):

- **Workspace-root count** — number of `Cargo.toml` files declaring `[workspace]`.
- **Disjoint-component count** — union-find over intra-workspace dependency edges.
- **Pattern histogram** — mass of each candidate pattern *kind*, treated co-equally:
  trait-impl, derive, attribute-macro, registration-macro, function-table registration.
  This set is open: detection must not assume `impl Trait for` is the universal signal.
- **Seam-type inventory** — counts of boundary markers actually present: FFI/`extern`,
  process-spawn, IPC/syscall surfaces, `no_std` crates, serialization-over-transport,
  trait-object registries.

Any threshold used downstream (what histogram shape counts as "peaked") is a **declared,
surfaced, overridable assumption** recorded in the fingerprint — never a validated constant,
because thresholds can only be calibrated against real repos the build cannot run.

### 4.2 Select mode

Ordered cascade — cheapest discriminating signal first:

1. The **histogram** decides the mode for most repos:
   - one kind dominates → **single-dominant**
   - a few kinds of comparable mass → **co-equal-few**
   - no kind above noise → **no-dominant**
2. **Structural signals escalate** only when the histogram fails to peak. Multiple
   workspace roots or disjoint components force **regional** regardless.

No graph clustering / community detection enters selection. Clustering only helps sub-cut a
single large connected component with no dominant pattern; disjoint components already give
regions for free, and where sub-cutting is needed, **seam-based cutting** (reusing the seam
inventory) yields a more meaningful partition than statistical modularity, because regions
are defined by their seams, not by dependency density.

### 4.3 Partition  *(regional mode only)*

- Disjoint components → top-level regions, for free.
- A monolithic-heterogeneous component → subdivide by **seam density**.
- A **mandatory seam-spine**: every region trace must terminate in *named* seams to other
  regions, and the top of the orientation is the join of those seams.
- If two regions share no traced data path, **say so** (honest UNRESOLVED) rather than
  padding the orientation to fake connectivity.

### 4.4 Trace

Pick one instance of the dominant pattern and follow it across the boundaries it actually
crosses. Stop **honestly** at the first boundary static tracing cannot cross —
macro-expansion, IPC, syscall — and emit an UNRESOLVED (§7) rather than narrating over the
gap. A stop is a success: it marks a real architectural seam worth human attention.

### 4.5 Emit  *(two files — locked)*

The split protects two *kinds* of artifact, not a literal file count:

- **Orientation (read-first, relational).** Authoring-rehydration order: crate/region map →
  core type vocabulary → seam-spine → data-flow narrative → **one worked slice (authoring
  template)** → UNRESOLVED guardrail list → pattern-authoring guide. The skeleton (map,
  vocabulary, seams, flow) comes first because architect-level work extends it; the slice is
  the integrative template threaded through that skeleton. Carries *relationships*; routes
  exhaustive depth elsewhere. **Must never approach source size.**
- **Reference (grep-into, exhaustive).** Every type/trait/impl/registration, span-anchored;
  regional depth; secondary-lens detail (runtime/error paths, lookups). Permitted to be
  arbitrarily large.

Regional depth is routed by *character*, not by region: regional *relationships* go in the
orientation (spine + map + pointers), regional *exhaustive detail* goes in the reference.
This is what keeps the orientation read-first even on an OS-scale tree and holds both locks
(two-file split *and* orientation-never-approaches-source) at once.

Output is **files on disk**, never a conversational summary.

## 5. Provenance header

Both files carry: commit hash, `rustc`/toolchain version, generator/tool version, and
whether the rustdoc overlay was present. This is snapshot labelling for reproducible,
diffable rebuilds — not staleness detection.

## 6. Item format (three-axis)

Every item gets **what / why / where**, 1–2 short sentences each:

- **what** — purpose: input/output/state and environment changes.  *(Load-bearing.)*
- **where** — where it sits in the flow.  *(Load-bearing.)*
- **why** — why it exists relative to *this* codebase.  *(Supporting.)*

Applied to crates, core types/traits, and the worked example. Leaf items may get **what +
span only**, with why/where marked `unverified`.

**Anti-fabrication on the why-axis — a defect-prevention rule.** `why` is the one axis no
tool can infer from syntax. Populate it only from doc-comments / README / module docs; where
absent, mark it `unverified` — never synthesize plausible rationale. The consumer writes
code: a fabricated rationale silently misdirects an architect-level design decision into a
wrong commit. The same caution applies wherever what/where cannot be determined — mark,
don't invent.

## 7. UNRESOLVED honesty — three levels

The honesty discipline escalates across the whole pipeline:

1. **Trace-level** — a data path stops at a macro/IPC/syscall boundary. Record *what I
   looked for* and *what I ran*. This is the **authoring guardrail list**: the next instance
   verifies against source before writing across the seam, rather than guessing a bridge
   that compiles but is wrong.
2. **Item-level** — why/where unverifiable → marked, never invented.
3. **Method-selection-level** — when the fingerprint is genuinely ambiguous between modes,
   report the choice *and its runner-up*, rather than silently committing.

No plausible-sounding fiction at any level. An honest UNRESOLVED is a success.

## 8. Span anchoring

Every generated claim points to a real `file:line` (range where natural). Spans are
**citations**: they make the architecture prose falsifiable against source, so machine docs
route to the source rather than replace it, and fabrication is self-correcting. A rehydrated
author follows a span into source and extends it directly — cheaper than re-reading prose —
so the orientation leans on spans-as-routing and carries minimal restated content, which is
also how it stays under source size. Spans that a semantic overlay reports as null —
re-exports, blanket/synthesized impls, macro-generated items — are **flagged, not dropped**.
