# Build-Session Scaffold

**Status:** guidance for the session that *builds* the skill. This is not the skill's
content and must not be encoded into the skill body. Read alongside `METHOD-SPEC.md`
(the what) — this file is the how, plus the constraints of the build environment.

---

## 1. Degradation-first is the primary design axis

Graceful degradation is not a fallback bolted on at the end; it is the axis the whole
generator is organized around. Every richer capability is a third-party or toolchain
dependency that may be absent. Therefore:

> The pure-stdlib floor must produce a **correct and honest** artifact on its own. Each
> available upgrade only makes the artifact **better** — never determines whether it is
> trustworthy.

This single principle resolves the substrate question, the clustering question, and the
rustdoc question consistently: anything you cannot guarantee is present is an enhancement,
not a foundation.

## 2. Detection substrate hierarchy

Three tiers, each optional above the floor:

1. **Floor — pure-Python fact scanner (stdlib only, always runs).** A bracket-, comment-,
   string-, raw-string-, and `cfg`-aware scanner emitting facts (impls, trait defs, derives,
   attribute-macro invocations, macro invocations, type defs, re-exports, registrations)
   with line spans. This is what fixes regex's miscounting of commented-out and `cfg`-d-out
   code. Note: `syn` is a *Rust* crate and is **not** an option here — using it would require
   a Rust toolchain and compilation, violating the stdlib-Python constraint. The floor is
   hand-written Python, not a binding to a Rust parser.
2. **Opportunistic — `tree-sitter-rust` (only if the wheel is present).** A real parse tree
   removes scanner edge-case risk. *Not assumable* — the build container has no network, and
   the run environment may also lack it. Detect availability; upgrade silently if present.
3. **Semantic overlay — rustdoc JSON (only if a nightly toolchain + buildable project
   exist).** Post-macro-expansion ground truth and re-export identity. `format_version`-
   guarded; on absence or version mismatch, degrade to floor output with an explicit banner.
   rustdoc fixes the **count and identity** of macro-generated items (e.g. registrations
   expanded from a macro that the scanner sees only as an invocation), while their **span**
   may be null or point at the call site — flag those, do not drop them.

The two static-vs-semantic tiers fail in opposite directions and reconcile cleanly: the
scanner over-trusts source text (sees commented/`cfg`-d-out as real, sees macro invocations
but not expansions); rustdoc over-trusts the compiler (loses spans, loses source-literal
structure). Disagreement between them is itself a seam signal worth surfacing.

## 3. Why regex is out (do not reintroduce it)

The prior design used regex for pattern detection. It is rejected on three independent
grounds, each addressed by a different part of the design:

- It miscounts commented-out and `cfg`-d-out impls → fixed by the comment/`cfg`-aware floor.
- It is blind to macro-generated impls → fixed by the rustdoc overlay.
- It assumes `impl Trait for` is the universal pattern signal → fixed by the **histogram of
  co-equal candidate kinds** (trait-impl, derive, attribute-macro, registration-macro,
  function-table). Some architectures express their dominant pattern as derives or
  registration macros, not trait impls.

Regex is not even retained as a pre-pass: a free generation budget means a pre-pass only
adds a reconciliation surface for no benefit.

## 4. Tooling language

Python; stdlib where possible. `Cargo.toml` parsing uses stdlib `tomllib` (3.11+).
Union-find for component counting is a few lines of stdlib Python. Only rustdoc extraction
may shell out (`cargo +nightly rustdoc … --output-format json`). Do **not** write nushell
scripts even when the target/runner shell is nushell — the tooling runs as Python there.

Clustering / community detection is excluded partly on the same ground as `tree-sitter`: it
needs a third-party library (or a hand-rolled algorithm with a resolution knob that merely
relocates the arbitrariness). Component counting (bedrock, union-find) stays; modularity does
not enter selection.

## 5. Build-environment limits — what can and cannot be executed here

The container has **no network, no nushell binary, no real cargo project.**

- **Deterministic phases** (scanner, characterization/fingerprint, mode selection, emit) are
  testable now and **must be unit-tested against synthetic Rust trees** — including the
  adversarial cases: commented-out impls, `cfg`-gated impls, raw strings containing
  impl-like text, macro invocations, multi-workspace layouts, disjoint components.
- **Rustdoc extraction and any real-repo trace cannot run in-container.** Spec them
  carefully and reason about them; do **not** pretend to validate them. Any threshold must
  be defensible from first principles, surfaced in the fingerprint, and overridable — never
  a constant dressed up as validated.

## 6. Quality bar — embody the property you preach

A procreative-rebuild artifact whose own tooling is sloppy, or whose orientation doc bloats,
is self-contradicting. The generator's code and the orientation it emits must both be tight
and hand-wave-free. Eat the dog food: if the orientation is allowed to grow, it must still be
a *map* (navigable read-first relationships), never a second source tree.

## 7. Corrections inherited from the design (so they are not re-litigated)

- `syn` is Rust, not Python — substrate floor is a hand-written Python scanner (§2).
- Clustering needs third-party libs and a resolution knob — excluded from selection (§4).
- The why-axis is the fabrication-risk axis — anti-fabrication on it is central, not
  cosmetic (see `METHOD-SPEC.md` §6).
- At least one repo-specific prior in the original brief was already wrong — repo facts are
  quarantined in `VALIDATION-FIXTURES.md` and must be verified, never trusted.
