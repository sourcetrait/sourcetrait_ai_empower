# template.rs

The three source builders. Each substitutes a NUON literal rather than leaving
the eval to re-parse JSON, so the parser downstream only ever sees well-formed
nu.

## const CALL_ALIAS
A constant WE own, and that ownership is the whole point. A call target's name is
the AGENT's to choose and may collide with a nushell KEYWORD, which can never be
shadowed - the keyword wins at parse under `use`, `overlay use` and
`overlay use --prefix` alike. `run` BECAME a keyword in 0.114 and there is a
`run` call target in the estate, so aliasing the overlay sidesteps the namespace
entirely and the drive is a constant rather than the target's own name.

## fn args_to_nuon

## fn json_object_to_nu_value

## fn json_value_to_nu_value
Also the write half of the rig index's NUON round-trip, which is why it is
`pub(crate)` rather than private to the templates. An out-of-range number
degrades to a string rather than failing, on the reasoning that a value the
agent sent is better preserved lossily than rejected.

## fn args_literal
Strict void falls OUT of this function rather than being enforced anywhere: a
void positional with empty args binds the bare `null`, while a void positional
with non-empty args emits the record NUON against a `nothing` parameter and
fails the positional typecheck. That is why the grammar maps a top-level `{}` to
`nothing` and not to `record<>` - an open record would accept the populated
record and void would not be void.

## fn build_run_source
The outer `do {}` keeps the synthesized def out of the eval engine's command
scope. Belt and braces rather than load-bearing, since each stateless eval runs
on a fresh clone that is dropped when it returns, so nothing could persist
regardless.

## fn build_call_source
Three pieces, each load-bearing under nu 0.114 and none of them obvious:

The overlaid path is the TARGET's own module, not the rig root. 0.114 (PR #18303)
stopped implicitly importing a module's submodules, so the pre-0.114 form - a
root `use` plus a module-qualified drive - binds nothing the drive can traverse
and is dead.

`as __call` sidesteps the keyword collision described under `CALL_ALIAS`.

`--prefix` matters because a call target MAY export helpers beside `main`.
Without it the overlay flattens them bare into the eval scope, where one named
`open` would shadow the builtin for everything else in that eval.

Importing the target alone rather than the whole rig costs nothing: the target's
own `use rig/...` self-references bind at THAT file's parse against the const
`$NU_LIB_DIRS`, never through this import.

## fn build_interact_source
The `( ... )` wrapper is NOT interchangeable with run's `do {}`, and choosing
wrongly silently breaks persistence: `do {}` would scope the env away, while the
parenthesized subexpression lets a `def --env` body's `$env` writes and `cd`
reach eval-top where the interact engine's merge persists them. The `;` after the
def is required inside `()`.

The `[args: ARGS_TYPE]` positional is what runtime-checks the args: a typed `let`
binding would enforce NOTHING at all, so a wrong-shaped record would slip through.
