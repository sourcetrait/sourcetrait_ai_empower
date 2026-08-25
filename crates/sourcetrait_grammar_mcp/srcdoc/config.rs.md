# config.rs

`*Toml` is the FILE layer - every field optional, paths as portable strings,
unknown keys rejected - and `Config` with its sub-items is the format-free
runtime layer. Sub-items get the same pair so a future format adds a shell
without touching the model.

THE FILE CARRIES ONLY WHAT IS NOT AN ARGUMENT. The store namespace (`--id`,
`--namespace`), the work dir and the deny list stay arguments: they IDENTIFY or
GATE the invocation rather than tune it. A file-settable `id` or `namespace` would
let the namespace silently diverge from the `.mcp.json` entry the agent believes it
is talking to, and a sticky default is worse than an explicit argument. So the two surfaces
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

## struct RemoteToml / struct RemotesConfigToml
The remotes.toml FILE layer, the sourcetrait split (config_toml_split). The file root
is `RemotesConfigToml { remote: Vec<RemoteToml> }` - a `[[remote]]` ARRAY, one entry
per peer, each `RemoteToml` a plain `deny_unknown_fields` struct (no flatten, no
self-acceptor subtable: the retired leg-4a/4c `[remote.<alias>]` shape + the
RemoteListen self-config are gone). The `<Foo>ConfigToml` suffix marks the FILE; a
nested entry drops it to `<Foo>Toml`. The operator's field names are FIXED and never
renamed for internal use (alias, listen, address, self_public_key_file,
self_private_key_file, public_key_file); a mistyped field is a loud
deny_unknown_fields error.

## struct RemoteConfig / struct RemotesConfig
The model layer, field names MIRRORING the toml (self_public_key_file /
self_private_key_file / peer_public_key_file - never cert / key / pin nouns in our own
names). `RemotesConfig { by_alias: HashMap }` lives in `Config.remotes`.

## impl TryFrom<RemoteToml> for RemoteConfig / impl TryFrom<RemotesConfigToml> for RemotesConfig
The bridge is TryFrom, the house convention, retiring the bespoke `merged_remote`. The
per-entry TryFrom resolves role from `listen` presence: a listener's `address` is a
BARE source-IP filter (parses to IpAddr, no port - a host:port form is rejected), a
connector's is the ip:port dial target (SocketAddr); the three key paths are required +
expanded at load, so a bad address or a missing key fails at STARTUP, not at bind. The
outer TryFrom rejects a duplicate alias. No embedded defaults (deployment-specific), so
an absent file is `RemotesConfig::default()`. The top Config's `from_toml` stays a
named constructor (NOT TryFrom) precisely because it carries fields the format layer
lacks (id, namespace); this sub-config, which does not, uses TryFrom.

## fn fraction_field
`pub(crate)` for one reason, and it is worth stating because the visibility looks
arbitrary otherwise: a runtime PIN is held to the same bound the file layer
enforces. A pin must not be able to reach a state a config load would have
refused, and the only way to guarantee that is to SHARE the check rather than
restate it.

Note the callers pass the BARE field name, not the dotted key - this function
composes the `supervisor.` prefix itself, so handing it an already-dotted key would
double it to `supervisor.supervisor.cpu_warn_fraction` in the error message. A test
that asserts refusal alone would miss that; the text has to be read back.

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

The expansion itself is `shellexpand::full` behind sourcetrait_common's guard
idiom (see agnostic's `XdgDir::homed`): a solved utility is never re-implemented
by hand, and the dependency's version spec mirrors common's (`"3"`, no
features). A hand-rolled predecessor died twice here: its `&str -> &Path` port
kept `strip_prefix("$")`, and `Path::strip_prefix` matches whole COMPONENTS, so
the `$` branch could never fire and `$VAR` paths passed through literally. The
retired `var_or_xdg` XDG fallbacks are gone deliberately: generic expansion
invents no values - XDG defaulting is `XdgDir`'s concern - so an unset variable
in a config path fails the load, which is the tested contract.
