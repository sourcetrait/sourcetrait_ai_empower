# Validation Fixtures — Test Corpus

> **DO NOT ENTER THE METHOD BODY.** Everything below is a prior or fixture for *validation
> runs only*. No repository-specific fact here may be encoded into the method spec or the
> skill body — doing so would defeat the procreative-rebuild property (a fresh instance must
> rederive these against the live tree, not inherit them).
>
> **Verify against current HEAD; do not trust.** At least one prior in the original design
> brief was already wrong: it called `Signature` a *trait* — it is a **struct** (builder
> pattern). That single error is proof the verify-don't-trust rule is load-bearing.

These four repositories are chosen because each **breaks a different assumption**, which is
what forces the method to stay general instead of overfitting to one codebase's shape.

---

## nushell — single-dominant, macro-mediated, one IPC seam

The baseline case the method must handle cleanly.

- **Shape:** one dominant pattern — `impl Command for X` — by overwhelming count.
  Registration is **macro-mediated** (a `bind_command!`-style macro inside an
  `add_shell_command_context`-style function). One genuine cross-process seam: plugins.
- **Corrected priors (verify):**
  - `Command` is a **trait**, ~`crates/nu-protocol/src/engine/command.rs`.
  - `Signature` is a **struct** (builder), *not* a trait — the brief erred.
  - Core types every crate speaks: `Value`, `PipelineData`, `EngineState`, `Stack`, `Span`,
    `ShellError`.
  - Built-in registration via a `bind_command!`-style macro in
    ~`crates/nu-command/src/default_context.rs`.
  - The plugin surface is a *separate* trait (`PluginCommand`, in `nu-plugin`) communicating
    over stdin/stdout serialization — same `Command`-shaped surface, across a process seam.
- **Exercises:** macro-blindness honesty (the scanner sees the registration macro
  *invocation* but not its *expansion* → count is UNRESOLVED without the rustdoc overlay);
  the single-dominant slice; the plugin IPC boundary as a *worked* trace-level UNRESOLVED.
- **Expected honest-failure points:** registration-macro expansion; the plugin IPC boundary.

## Bevy — breaks "`impl Trait for` is the signal"

The repo that most forcefully justifies the open candidate-kind histogram.

- **Shape:** the dominant *authored* pattern is **derive + function**, not a trait impl:
  `#[derive(Component)]` / `#[derive(Resource)]` plus **systems as plain functions**
  registered via `IntoSystem`-style blanket impls over function signatures. The instance you
  trace is *a function + its registration + its data dependencies*, not an `impl` block.
- **Exercises:** derive and function-table/registration as first-class candidate kinds; a
  detector keyed on `impl Trait for` would miss the dominant pattern entirely.
- **Depth is rustdoc-contingent:** blanket impls (`IntoSystem` machinery) and derive macros
  are exactly where the floor scanner bottoms out at UNRESOLVED; the rustdoc overlay adds
  real depth here. So Bevy depth varies with whether the run environment has a nightly
  toolchain — the degradation axis, not a defect.

## Helix — tests function-table command registration (discover, don't assert)

- **Shape:** plausibly **function-table-dominant** — typed command *functions* registered in
  a static table via a macro — a third pattern kind, distinct from nushell's trait-impl and
  Bevy's function-as-system. It may instead be genuinely heterogeneous.
- **Do not assert which.** Let the characterization pass *measure* whether Helix is
  function-table-dominant or no-dominant. (An earlier draft over-claimed Helix as the
  "no-dominant" exemplar — that was an assertion the tool should make, not the designer.)
- **Seams:** tree-sitter FFI; LSP IPC.
- **Exercises:** function-table detection; the discover-don't-assert discipline at the
  method-selection level.

## Redox — breaks "one cargo workspace"

The repo that forces regional mode and multi-workspace discovery.

- **Shape:** an OS — **multiple workspaces / disjoint dependency graphs**, microkernel plus
  userspace, `no_std`, custom targets, and scheme/syscall seams pervasively. Not a single
  clonable cargo workspace; assembled from many crates.
- **Exercises:** multi-workspace crate discovery; **regional mode** with the mandatory
  seam-spine; disjoint components → top-level regions for free; **seam-density subdivision**
  of a monolithic component (e.g. userspace, if it forms one connected blob) — where
  seam-cutting must beat statistical clustering. The region unit is *measured*, not fixed.
- **Watch:** the orientation must stay a *map* at OS scale — regional **relationships** in
  the orientation, regional **exhaustive detail** routed to the reference. Absolute size is
  fine; loss of navigability is not.

---

## What each repo exercises (quick map)

- **nushell** → macro-expansion + IPC UNRESOLVED; single-dominant slice.
- **Bevy** → non-`impl` candidate kinds (derive, function); rustdoc-dependent trace depth.
- **Helix** → function-table detection; discover-don't-assert at selection level.
- **Redox** → multi-workspace discovery; regional mode; seam-based region cutting.

## Run-environment note

Validation runs require a real checkout and (for the overlay) a nightly toolchain — none
available in the build container. The deterministic phases can be exercised on synthetic
trees that *imitate* each shape above (a synthetic multi-workspace tree for Redox, a
synthetic derive+blanket-impl tree for Bevy, a synthetic registration-macro tree for
nushell/Helix), but the real traces and rustdoc extraction must be run where a toolchain and
network exist.
