# Method reference - tracing and filling the orientation

This is the depth behind Step 4 of `SKILL.md`. Read it when you are filling the `[AGENT]`
sections and want the reasoning, not just the rule.

## Why a worked slice, and why only one

A large workspace is intimidating because it looks like thousands of unique things. It is
almost never that. It is a few *kinds* of thing, instantiated many times. The single highest-
leverage act of orientation is to identify the dominant kind and trace exactly one instance of
it all the way through - across crate boundaries, through registration, to where its output
goes. That one trace is worth more than summaries of fifty crates, because it is *executable*:
the next time you author an instance of that pattern, you follow the same path.

Trace one, not three. A second slice rarely teaches a new boundary; it dilutes the protagonist
and inflates the map. If a second pattern is genuinely co-equal (the histogram shows two close
leaders), trace the second only far enough to show where it *differs* from the first.

## The dominant pattern is whatever the histogram says

`fingerprint.json`'s `pattern_histogram` ranks candidate kinds empirically:

- `trait_impl:<Trait>` - the classic "implement this trait" extension point.
- `derive:<Trait>` - behaviour attached by `#[derive(...)]` (e.g. an ECS `Component`).
- `attr_macro:<path>` - behaviour attached by an attribute macro.
- `reg_macro:<name>` - items registered through a macro; the count is the call-site argument
  count, and it is **unverified** until rustdoc confirms the expansion.
- `fn_table:<crate>` - behaviour expressed as many free functions (e.g. a command table).

Do not walk in assuming `impl Trait for`. Bevy's dominant pattern is a derive; Helix's may be
a function table; nushell's is a trait impl cross-confirmed by a registration macro. Let the
ranking decide, and when two kinds nearly tie, treat both as load-bearing.

## The three axes, and why their weights differ

Every item you write is three axes: **what / where / why.**

- **what** - what it does: inputs, outputs, the state and environment it reads or changes.
- **where** - where it plugs in: how it is registered, how it is invoked, what calls it.
- **why** - why it is shaped this way.

*What* and *where* are load-bearing because the reader is going to write code that depends on
them. Get the inputs/outputs wrong and their code is wrong; get the registration path wrong and
their new instance never runs. *Why* is supporting: it speeds comprehension but the reader does
not compile against it.

Keep each axis to 1-2 sentences. The orientation earns its read-first status by being dense
and relational, not by being long.

## Anti-fabrication on the why-axis

This is the rule that most protects the reader, because they write code. A plausible-but-wrong
*why* is worse than a blank one: it justifies a change that should not be made.

State *why* only from something you can point at - a doc-comment, a clearly-named invariant,
an obvious code constraint. If you cannot verify it in source, write `why: unverified`. The
emitter already stamps undocumented items that way; do not overwrite the stamp with a guess.
"I don't know why this is shaped this way" is a true, useful statement. An invented rationale
is a latent defect you are handing to the next author.

## UNRESOLVED as an authoring guardrail

A trace that stops honestly is a trace that succeeded at finding a boundary. Record every stop
as `UNRESOLVED: <what you looked for>, <what you ran>` (e.g.
`UNRESOLVED: how bind_command! expands these into the registry; read default_context.rs, ran
grep for the macro_rules! definition, expansion not statically resolvable`).

You will hit these boundaries:

- **Macro expansion.** The scanner sees `bind_command!(A, B, C)` and captures the arguments,
  but the expansion is invisible to it. Confirm the generated items in source or via the
  rustdoc overlay before you rely on the registry. Until then, the count is unverified.
- **Process / IPC boundary.** A child process or a stdin/stdout protocol crosses an address
  space. Find the wire format in source if you can; if not, stop and mark it. Do not assume the
  far side's shape.
- **FFI / syscall boundary.** `extern` and `libc` cross into non-Rust or the kernel. Verify the
  foreign contract; do not infer it.
- **Dynamic dispatch registries.** `Box<dyn Trait>` collected at runtime cannot be enumerated
  statically. Note the registration site and that membership is dynamic.

The reason this matters for *authoring*: a guessed bridge across one of these seams compiles
and then misbehaves. The UNRESOLVED marker is the instruction "do not write across this seam
until you have read both sides."

## Regional workspaces

When `selection.mode` is `regional`, the workspace is either multiple Cargo workspaces or
multiple disjoint dependency components (the tool finds these via union-find - no clustering
heuristic, no resolution knob). Each component is a region.

- Put each region's *relationships* (its role, the named seams joining it to others) in the
  orientation. Put each region's *exhaustive detail* in the reference. This character-based
  routing is what keeps the orientation read-first even at OS scale.
- The seam-spine is the join between regions. If two regions share **no** traced data path,
  say so as an UNRESOLVED - do not invent a connection to make the map look whole. A kernel and
  a userspace shell may communicate only through syscalls; that *is* the relationship, and the
  syscall edge is the seam.
- The dominant pattern still holds within a region even when structure forced the regional
  mode. The `selection.notes` will tell you when the histogram alone would have picked a
  non-regional mode.

## Spans are the routing and the honesty mechanism

Every claim resolves to `file:line`. This is not decoration:

- It is the route back into source - the orientation's whole job is to send the reader to the
  right place to author.
- It is self-correcting. A fabricated claim has no real span, or a span that does not say what
  the claim says. Forcing yourself to cite is forcing yourself to verify.

When the rustdoc overlay marks a span null (re-exports, blanket/synthesized/macro-generated
items), keep the entry and keep the null flag. A null span is information: it says "this exists
but has no single source location" - which is itself a fact about how the item came to be.

## The finished orientation, checked with fresh eyes

Before you call it done, re-read `orientation.md` as if you were the next agent:

- Could you author a new instance of the dominant pattern from S5 + S7 alone, opening only the
  spans cited? If not, the worked slice is incomplete.
- Is every *why* either sourced or marked `unverified`? No orphan rationales.
- Is every boundary either traced or marked `UNRESOLVED`? No narrated gaps.
- Did the map stay small? If S1-S7 read like a second copy of the source, detail leaked out of
  the reference and into the map - push it back.
