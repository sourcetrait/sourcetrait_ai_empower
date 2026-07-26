# cli.rs

## struct HostCli
ONE BINARY, with the variant selected at runtime by trusted operator config -
which is why there is no build feature for a test host and no second target.

The validation here is deliberately UNEVEN, and the unevenness is the design.
`id` and `namespace` are NOT ident-checked: the_user owns the `.mcp.json`
entries, and a bad value surfaces as the natural downstream error rather than
being pre-empted here. `workdir` is expanded but never existence-checked, on the
same trust. `--deny` IS validated, because a typo silently denying nothing would
defeat the operator's whole intent - the failure mode differs in kind from the
others, so the treatment does.

`namespace` is `Option<String>` rather than a clap `default_value`, and that is
load-bearing rather than stylistic: with a default, clap cannot distinguish an
explicitly-passed value from a defaulted one, and `--test` needs exactly that
distinction to supply a default without overriding an explicit choice.

The parse error in `parse_deniable` carries the full deniable list, which is why
the `--deny` help text no longer repeats it. That is the general shape of what
the summary cap costs here: duplication, not information.

## enum CliTool
Deny does NOT apply to this surface. It gates agent REGISTRATION, and the
one-shot path dispatches straight to the handler methods without the router, so
the operator always has the whole set.

The consumer of this output is a WRAPPER, never a human eye - a nu-native front
that takes real records and serializes at this argv boundary - which is why
record-shaped inputs arrive as single-quoted NUON strings and the output is bare
compact JSON with no tty branch anywhere.

## fn resolve_namespace
The namespace half of `--test` ENDS HERE, and that is what keeps the flag from
becoming a mode. From this function onward `test` is an ordinary namespace
string, so nothing downstream re-derives it, which leaves `Config.test` meaning
only the watchdog half.

## fn resolve_work_dir
Every path out of an argument expands through the same `expand_path` the config
file uses, so a `~` or a `$VAR` means the same thing whichever surface it
arrived on.

## fn resolve_config
The two surfaces are DISJOINT by design, so there is no precedence to resolve
between flag and file. What makes that safe rather than fragile is
`deny_unknown_fields` on the file layer: a file reaching for an argument is a
loud error instead of a silent no-op.

## fn host_main
CONSTRUCTED BY HAND rather than through `#[tokio::main]` for exactly one reason:
`thread_stack_size`.

Nushell PARSING is deeply recursive, and two parses run inline on a runtime
worker rather than on an eval thread - the body lint and the rig validator. A
pathological module graph, whose resolution recurses until the accumulated path
stops resolving, overflows the 2 MB worker default and ABORTS the process. A
stack overflow is a fatal runtime error rather than a panic, so `catch_unwind`
cannot save it and the host dies leaving no diagnostic at all. That is what made
it read as infrastructure rather than as code, and cost a long diagnosis.

Sizing every worker like the eval thread removes the CLASS rather than one
instance: the recursion is bounded by the filesystem's path limit, so its depth
has a finite ceiling whatever module graph reaches it. Doing it at the RUNTIME
rather than per call site is deliberate - a per-site fix has to be applied
everywhere a parse can happen, and missing one leaves a latent host death.

The `tk::spawn` is not decoration either. `block_on` drives the future on the
thread that called it - `main`, whose stack `thread_stack_size` does not
govern - so without the spawn the serve path would get the sizing while the
one-shot CLI parsed on whatever the process was given. That gap survives in a
release build and core-dumps in a debug one, because unoptimized frames are
fatter, which is precisely the kind of difference that hides until it matters.

## fn serve_or_oneshot
`CONFIG` is set before serve or one-shot dispatch, and before any reader, because
`config()` panics when read early - a startup-order bug rather than a runtime
condition.

Exit code 2 is the operator-input code the one-shot CLI already uses, so a
rejected config and an unparseable argument report the same way.
