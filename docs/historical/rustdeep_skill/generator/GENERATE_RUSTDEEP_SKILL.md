# REBUILD — generator for the `repo-orientation` Claude skill

**Purpose.** Hand this file, plus the input archive(s) described below, to a fresh Claude
session (Claude Code, or any session with code execution and a filesystem) to regenerate the
`repo-orientation` skill from scratch — faithfully, and verifiably. This is the reproducible
*process* behind the frozen artifact; check it in so the skill is never an opaque blob. A
correct run ends with a self-test printing `ALL 11 TESTS PASSED` and a packaged skill folder.

You (the rebuilding session) have no memory of how this skill was first built. Everything you
need is in this prompt and the uploaded archive(s). Read this whole file before writing code.

---

## 1. Inputs — upload alongside this prompt

You will be given **one or both** of the following. Use whichever are present; the procedure
degrades gracefully. The built-skill archive is the minimum and is strongly preferred because
it carries the objective acceptance test.

- **`repo-orientation/` — the built skill (PREFERRED; carries the test oracle).**
  Reference implementation: `SKILL.md`, `scripts/{rustscan,characterize,emit,rustdoc_overlay,
  test_orientation}.py`, `references/method.md`. **Authoritative on structure and behavior.**
  Its `scripts/test_orientation.py` is the acceptance test: a correct rebuild makes it print
  `ALL 11 TESTS PASSED`.

- **Design specs — `METHOD-SPEC.md`, `BUILD-SCAFFOLD.md`, `VALIDATION-FIXTURES.md`.**
  **Authoritative on intent, contract, and rationale** — the *why*, the locked design
  decisions, the build-environment constraints, and the validation corpus.
  `VALIDATION-FIXTURES.md` is *quarantined design data*: it informs the tests, but its
  repo-specific facts (nushell/bevy/helix/redox priors) must **never** leak into the shipped
  skill body. The skill must stay repo-agnostic.

**Precedence when sources differ:** the **test suite arbitrates behavior**; the **specs
arbitrate intent and why**; the **reference source is the structure to reproduce**. If the
reference source contradicts a spec on rationale, the spec wins and you re-derive. With specs
only: rebuild from intent and reconstruct the tests from §6 below + `VALIDATION-FIXTURES.md`.
With the built skill only: reproduce it and make its tests pass (rationale is weaker but
behavior is pinned). This prompt is self-contained enough to rebuild from it alone if needed.

---

## 2. What you are building

A Claude **skill** (SKILL.md + bundled Python tooling) that writes a durable, span-anchored
**architectural orientation** for a large Rust workspace, anchored to the current commit. Its
output is two files on disk, not a chat answer:

- `<repo>/.orientation/ORIENTATION.md` — read-first map: crate/region structure, core type
  vocabulary, the seam-spine, a data-flow narrative, and ONE worked vertical slice of the
  workspace's dominant repeated pattern traced across crate boundaries. Small and relational.
- `<repo>/.orientation/REFERENCE.md` — exhaustive, span-anchored index. Grep into it.

**The reader is a future agent** (often a Claude Code instance after a clean session or a
compaction) that must **author architect-level features into the repo** without re-deriving
the architecture. Optimize every section for *authoring*.

**The method the skill embodies — and must itself exhibit:** *many instances of few kinds.*
Find the workspace's dominant repeated pattern empirically, trace one instance as a worked
slice, and let that slice be the template for authoring the next instance. The tooling must be
clean and the orientation must not bloat — the skill should practice what it preaches.

---

## 3. Non-negotiable invariants (the contract DNA)

**Output contract.** Two files. Orientation is read-first and never approaches source size.
Routing is by *character*, not region: regional *relationships* (seam-spine, map, pointers) go
in the orientation; regional *exhaustive detail* goes in the reference. This holds even at OS
scale.

**Core method.** Dominant pattern is found empirically. Trace exactly ONE instance across
crate boundaries as the worked slice (the protagonist). A second pattern is traced only if
genuinely co-equal, and only far enough to show where it differs.

**Detection substrate (degradation-first).**
- *Floor* = pure-stdlib, structure-aware scanner. A character-level lexer masks comment bodies
  and string/char/raw-string *contents* (preserving byte and line offsets) so constructs
  inside comments/strings cannot miscount — the specific failure of naive regex scanning. Doc-
  comment text is preserved for the why-axis. It is **NOT** regex pattern-detection and **NOT**
  a `syn` binding (`syn` is a Rust crate; it would need a toolchain). `re` may be used only as
  a low-level lexer over already-masked text, never as the pattern detector.
- *Opportunistic* = tree-sitter-rust if a wheel is importable (optional).
- *Overlay* = rustdoc JSON if a nightly toolchain + buildable project exist; `format_version`-
  guarded; degrades to an explicit `status: absent` banner otherwise. Only this phase may shell
  out to cargo. The floor must produce a correct, honest artifact entirely on its own.

**Pattern model (co-equal kinds; never assume `impl Trait for`).** The histogram ranks
candidate kinds empirically: `trait_impl:<Trait>`, `derive:<Trait>`, `attr_macro:<path>`,
`reg_macro:<name>` (counted from call-site arguments; flagged `expansion_unverified` until
rustdoc confirms), `fn_table:<crate>` (heuristic). Bevy's dominant pattern is a derive, not an
impl — a detector that assumes `impl Trait for` is wrong.

**Pipeline.** characterize → select → (partition if regional) → trace → emit. The
**fingerprint is written to disk FIRST**, before any costly trace, so the chosen mode is
auditable. Disjoint dependency components are found by **union-find** (no clustering, no
modularity, no resolution knob); each component is a region. Regional mode is also forced when
there is more than one Cargo workspace root.

**Honesty mechanisms.**
- *Three axes per item* — what / where / why, 1–2 sentences. **what** and **where** are
  load-bearing (the reader writes code against them); **why** is supporting.
- *Anti-fabrication on the why-axis* — state *why* only from doc-comments or unambiguous code;
  otherwise write `why: unverified`. A wrong *why* becomes a real defect in the reader's code.
- *UNRESOLVED at three levels* — trace-level (stop at macro/IPC/FFI/syscall/dyn boundaries with
  `UNRESOLVED: <what you looked for>, <what you ran>`), item-level (`why: unverified`), and
  method-selection-level (report the runner-up mode when near a threshold, and note when
  structure forced `regional` over what the histogram alone would pick). Never narrate a gap.
- *Span anchoring* — every claim resolves to `file:line`. Null spans (re-exports, blanket /
  synthesized / macro-generated items under the overlay) are **flagged, not dropped**.

**Thresholds are declared defaults, not validated constants** — surfaced in the fingerprint and
overridable via `ORIENT_*` env vars: `DOMINANCE_SHARE=0.45`, `COEQUAL_TOPK=4`,
`COEQUAL_SHARE=0.60`, `AMBIGUOUS_BAND=0.07`, `SEAM_DENSE_PER_KLOC=4.0`.

**Provenance header** on both files: commit, rustc/toolchain, tool version,
`rustdoc_overlay_present` flag. Staleness is acceptable *because* the artifact says exactly
which commit it describes.

**Division of labor.** The Python tooling does the fabrication-proof work (counts, spans, crate
graph, components, pattern histogram, seam detection, mode selection). The agent does the
judgment work (the trace, the data-flow narrative, the why-axis) — but always anchored to the
tool's spans. The emitted ORIENTATION.md is a skeleton with `[AGENT]` slots; everything else is
already grounded.

**Scope / cost.** Generation cost is ignored (the reference is exhaustive). Campaign/progress
state is out of scope. SKILL.md stays under 500 lines and explains *why* rather than stacking
imperative MUSTs (progressive disclosure: depth lives in `references/method.md`).

---

## 4. Deliverable (exact tree; package clean — no `__pycache__`, no `.pyc`)

```
repo-orientation/
├── SKILL.md                     # frontmatter (name: repo-orientation; pushy description) +
│                                #   workflow + division of labor + the three authoring contracts
├── scripts/
│   ├── rustscan.py              # structure-aware fact scanner (the floor); imported by characterize
│   ├── characterize.py          # crate graph + union-find components + pattern histogram +
│   │                            #   seam inventory + mode selection; writes fingerprint.json FIRST,
│   │                            #   then facts.json
│   ├── emit.py                  # REFERENCE.md (exhaustive, span-anchored) + ORIENTATION.md
│   │                            #   (map-first skeleton with seeded [AGENT] slots)
│   ├── rustdoc_overlay.py       # optional semantic overlay; degrades to a banner without nightly
│   └── test_orientation.py      # the 11-test acceptance suite (runs in-container on synthetic trees)
└── references/
    └── method.md                # deep trace/emit guidance for the [AGENT] judgment work
```

### What each script must contain (reproduce from the reference; this is the spec if absent)

- **`rustscan.py`** — `mask_and_collect(src)` (the lexer; handles `// /// //! /* */ /*! */`
  nested block comments, `"..."`, `r"..."`/`r#"…"#`/`br"…"`, char-vs-lifetime `'a` vs `'x'`).
  `scan_file(relpath, src)` returns facts: impls (trait-for vs inherent, generics, cfg-gating,
  end_line), traits, types (struct/enum/union/type, with doc), fns (with brace_depth + doc),
  uses (re-export flag), attrs, derives, macro invocations **with argument lists**
  (`expansion_unverified`), macro_defs, and a seam marker tally. Item detection is
  qualifier-aware (a doc comment before `pub`/`pub(crate)`/`unsafe`/`async`/`const`/`extern`
  associates to the item). RPIT (`-> impl Trait`) is **not** counted as an impl.
- **`characterize.py`** — `find_crates` via `tomllib` (workspace roots + packages + internal
  deps), `UnionFind` → components, `scan_crate` aggregation, `pattern_histogram` (the co-equal
  kinds above), `select_mode` (`single_dominant` / `co_equal_few` / `no_dominant` / `regional`)
  with the declared thresholds and the two method-selection notes (ambiguity band; structural
  escalation), seam inventory + density. Writes `fingerprint.json` first, then `facts.json`.
  CLI: `characterize.py <repo_root> [out_dir]`.
- **`emit.py`** — `emit_reference` (per-crate, span-anchored: traits/types/impls/free-fns/
  macro-applications/re-exports; cfg-gated and null spans flagged). `emit_orientation`
  (provenance header; "how this artifact was shaped" with method-selection UNRESOLVED;
  §1 crate/region map; §2 core vocabulary = most-depended-on crate's public types with
  doc-or-`unverified`; §3 seam-spine; §4 data-flow `[AGENT]`; §5 worked slice seeded with the
  dominant pattern, one candidate instance, and all sibling spans; §6 UNRESOLVED guardrails
  seeded from detected seams + registration macros; §7 pattern-authoring guide `[AGENT]`;
  appendix histogram). `provenance()` shells `git`/`rustc`, degrading to `UNKNOWN`. Prints the
  orientation/reference line ratio. CLI: `emit.py <repo_root> [out_dir]`.
- **`rustdoc_overlay.py`** — `toolchain_available()`; `run_rustdoc_json` (shells
  `cargo +nightly rustdoc -- -Z unstable-options --output-format json`; cannot run offline);
  `reconcile(floor_facts, rustdoc)` resolving macro-generated item counts/identity, re-export
  targets, null-span flags, and floor/rustdoc disagreements (a disagreement confirms a seam);
  `KNOWN_FORMAT_VERSIONS` guard. Writes `{"status":"absent"}` and returns 0 when no cargo.
  CLI: `rustdoc_overlay.py <repo_root> [out_dir] [package]`.

---

## 5. Build & verify procedure

1. Build in a scratch dir (e.g. `/home/<user>/repo-orientation/`); copy the final, clean tree
   to the outputs location only at the end.
2. Write `rustscan.py` first and **test the lexer against adversarial input before anything
   else** (see §6 case 1) — masking correctness is the foundation; a bug here silently
   corrupts every downstream count.
3. Build `characterize.py`, then `emit.py`, then `rustdoc_overlay.py`.
4. Write `test_orientation.py` and **build the synthetic Rust trees the tests need** (§6).
   This environment has **no network, no cargo/nightly, and no real repo**, so: the
   deterministic phases (scan → characterize → emit) are unit-tested on synthetic trees here;
   the rustdoc overlay and a real-repo trace are *specified and implemented but not executed*
   in-container — only the overlay's **degradation path** is asserted.
5. Run `python3 scripts/test_orientation.py`. Iterate until it prints `ALL 11 TESTS PASSED`.
6. Run the floor end-to-end on a synthetic tree (`characterize.py` then `emit.py` then
   `rustdoc_overlay.py`) and confirm: two files emit with real spans, `[AGENT]` slots are
   present, the dominant pattern is surfaced, the overlay writes a `status: absent` banner, and
   the orientation/reference ratio is well under 1.
7. `py_compile` all scripts; confirm SKILL.md < 500 lines; remove `__pycache__`/`.pyc`; package.

---

## 6. The 11 acceptance tests (reconstruct exactly these; names are the contract)

Run in-container against synthetic trees. If you have the built skill, use its
`test_orientation.py` verbatim. If rebuilding from specs, recreate these:

1. `test_masking_excludes_decoys` — three decoy impls (in a `//` comment, a `/* */` block, and
   inside a `"string"`) plus a `'static` lifetime and a `'}'` char literal; only the one real
   `impl ... for Real` is counted.
2. `test_macro_args_captured` — `bind_command!(ws, A, B, C)` captures arg idents
   `[ws, A, B, C]` and is flagged `expansion_unverified`.
3. `test_derive_and_attr_macro` — `#[derive(Debug, Clone)]` yields derive entries; `#[tokio::main]`
   is recorded as an attribute-macro, not a derive.
4. `test_impl_for_vs_inherent_and_generics` — `impl Foo {}` is inherent; `impl<T: Send> Bar<T>
   for Baz<T> where T: Clone {}` is a trait impl `(Bar, Baz)`.
5. `test_rpit_not_counted_as_impl` — `fn make() -> impl Iterator<…>` yields zero impls.
6. `test_doc_for_why_axis` — a `///` doc before `pub struct Documented;` attaches to it; a bare
   struct has empty doc (the why-axis is sourced, not invented).
7. `test_nushell_shape_single_dominant` — workspace `p` (`trait Command`, `struct Value`) + `c`
   (≥25 `impl Command for Ci`, plus a `bind_command!` call) → dominant `trait_impl:Command`,
   mode `single_dominant`, 1 component.
8. `test_bevy_shape_derive_coequal` — workspace `ecs` (`trait Component`) + `game` (≥20
   `#[derive(Component)] struct Posi`, ≥20 free `fn system_i`) → derive mass ≥20 and the top
   pattern is `derive:` or `fn_table:`, **never** an `impl Trait for`. (This guards the
   "don't assume impl-Trait-for" invariant.)
9. `test_regional_multiworkspace` — a `kernel` workspace (`#![no_std]`, `extern "C"` syscall) +
   a separate `user` workspace (member `a` with a trait+impl) → mode `regional`, ≥2 components,
   and a `selection.notes` entry mentioning structural escalation.
10. `test_emit_two_files_and_spans` — emit produces ORIENTATION.md (contains `[AGENT]`, a
    "Worked slice" section, and the dominant pattern) and REFERENCE.md (contains real
    `file:line` spans).
11. `test_overlay_degrades_without_toolchain` — with an empty `PATH` (no cargo), the overlay
    exits 0 and writes `{"status": "absent", …}`; the floor artifact stands.

**Definition of done:** all 11 pass; SKILL.md < 500 lines; the floor runs with no network and
no cargo; the overlay degrades cleanly; the orientation stays far smaller than source. None of
the four exemplar repos' specific facts appear in SKILL.md or the scripts (quarantine intact).

---

## 7. Fidelity & voice

Reproduce the *architecture and contract*, not a loose reinterpretation. The skill must embody
the property it preaches: the tooling is clean and structure-aware (no sloppy regex
pattern-matching), and the orientation it emits is dense and relational, never a second copy of
the source. Assume the latest stable Rust and latest crate versions. Write for a technically
precise reader: concise, direct, no repeated disclaimers or hedging. SKILL.md teaches by
explaining *why*, with depth deferred to `references/method.md`.

**First real validation (note for whoever runs the rebuilt skill):** the rustdoc overlay's
live cargo invocation and the agentic worked-slice trace are specified but unexecuted in any
build container. The first run against a real repo (e.g. nushell) is what exercises both — the
floor will already produce a correct artifact; the overlay and trace get their first live test
there.
