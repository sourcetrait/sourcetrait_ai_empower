# config.rs

`*Toml` is the FILE layer - every field optional, paths as portable strings,
unknown keys rejected - and `Config` with its sub-items is the format-free
runtime layer. Sub-items get the same pair so a future format adds a shell
without touching the model.

THE FILE CARRIES ONLY WHAT WAS NEVER AN ARGUMENT. The store namespace (`--id`,
`--namespace`), the work dir and the deny list stay arguments: they were
arguments before this file existed, and they IDENTIFY or GATE the invocation
rather than tune it. A file-settable `id` or `namespace` would let the namespace
silently diverge from the `.mcp.json` entry the agent believes it is talking to,
and would reintroduce the sticky default already ruled out. So the two surfaces
are DISJOINT - there is no precedence question between them at all - and
`deny_unknown_fields` turns an attempt to set one from the file into a loud error
rather than a silent no-op.

## const TEST_NAMESPACE
A DEFAULT, not an override, and the distinction is the whole discipline around
`--test`: the flag names the ordinary test channel without spelling its namespace
out, and says nothing about a run that asks for a different one.

## const MIN_CHANNEL_PORT
Anything below 1024 is privileged and the host is unprivileged by design, so such
a bind could only ever fail. Rejecting it at LOAD turns a confusing permission
error at `channel_open` into a legible config error at startup.

## struct ChannelConfigToml
The port and the cert dir are the only two structural keys, because everything
else about the channel is fixed by design rather than configured. It is a
loopback socket serving a single host, so there is no address to choose and no
peer policy to express - the bind is 127.0.0.1 by construction, which is what
makes "only localhost gets in" a property of the socket rather than a rule
something has to enforce.

### field port
Modelled as an `Option` rather than a 0 sentinel because 0 is not a port. Zero is
rejected outright rather than treated as "any".

### field spam_warn_window_secs
Windows are INTEGER SECONDS on this surface for two independent reasons: MCP args
cross as JSON, which cannot carry a nu `duration`, and TOML has no duration type
either. The `10sec` form lives in the model and the docs, never on a wire or in a
file.

## struct Config

### field test
A binary flag with exactly two meanings, and deliberately NOT a mode: it defaults
the namespace to `test` - resolved at the argument boundary, so nothing
downstream re-reads it for that - and it ties the watchdog to the channel's
phase. Everything else behaves identically.

## struct ChannelConfig

### field spam
Runtime changes go through `config_channel`, which mutates the CHANNEL's live
copy rather than this. `CONFIG` is set once, so anything adjustable at runtime
needs its own live layer; the pin registry is the same shape applied to
`[supervisor]`.

## struct SpamThresholds
Two INDEPENDENT (window, rate) pairs so warn and error can measure different
things - a short window catches a burst, a longer one catches sustained
misbehaviour.

THESE VALUES ARE INITIAL. There is no basis for them beyond reasoning; only
production traffic will say what a normal producer actually does. The gap between
legitimate and runaway is enormous rather than marginal - a state lane emits
single digits per burst, a loop emits thousands per second - so the numbers barely
affect DETECTION and mostly decide how often a well-behaved fast command gets
flagged. That is why erring permissive is right here.

## fn fraction_field
`pub(crate)` for one reason, and it is worth stating because the visibility looks
arbitrary otherwise: a runtime PIN is held to the same bound the file layer
enforces. A pin must not be able to reach a state a config load would have
refused, and the only way to guarantee that is to SHARE the check rather than
restate it.

Note the callers pass the BARE field name, not the dotted key - this function
composes the `supervisor.` prefix itself. Handing it an already-dotted key
produced `supervisor.supervisor.cpu_warn_fraction` in a live error message, and
the unit test missed it because it asserted only that the value was refused and
never read the text back.

## fn secs_field
Zero would mean "no window", which is not a rate at all.

## fn rate_field
Zero would forbid the first send outright rather than police a rate.

## fn from_toml
Deliberately NOT a `TryFrom`: the model carries fields the format layer does not,
and a named constructor that takes them keeps that asymmetry visible rather than
hiding it behind a conversion.

## fn read_toml
An explicit `--config` that is absent or malformed is an ERROR rather than a
fallback to the defaults, because a typo'd path must fail rather than quietly
serve something else.

## fn default_id
A one-time process read for the zero-config human case. Harness `.mcp.json`
entries always pass `--id` explicitly, so this default is never what an agent
runs under.

## fn expand_path
Expanding at the BOUNDARY is what makes an unresolvable variable a load-time
error naming the variable, rather than a puzzle at first use much later.
