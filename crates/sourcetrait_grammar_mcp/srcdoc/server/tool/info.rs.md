# info.rs

`info()` exists because the rmcp init handshake's `serverInfo` is not relayed into agent
context by Claude Code. The tool surfaces the same facts through `tools/call` - including
the LIVE callable surface, so no stale skill index is needed.

## struct InfoParams

### field purviews
THE SUBAGENT BLINDERS. Ids given explicitly render as if those were in view WITHOUT
changing what actually is, which is what lets one session hand a subagent a narrower
surface at bootstrap while keeping its own.

EMPTY means CURRENT, never `default`. Those differ the moment a session has extended its
view, and defaulting to `default` would silently narrow a caller that passed nothing.

## struct InfoEnvelope

### field id
The agent's self-confirmation channel for which namespace it is talking to. The same pair
is readable ambiently inside a body as `$env.EQUIP_ID` / `$env.EQUIP_NAMESPACE`, so this
is the OUTSIDE view of the same fact.

### field mcp_nom
A CHANGED value across two calls is how an agent learns the server RESTARTED. A pid cannot
serve that, because pids are reused.

### field nu_version
Baked by `build.rs` from the workspace lockfile's `nu-protocol` pin, so it reports what is
actually LINKED rather than what a manifest asked for.

### field signatures
Read straight from each rig's committed index - no `.nu` walk and no parse - which is what
makes the agent's first read after the skill cheap.

### field purview
Last in the struct because it is the FRAME around the block rather than part of it.

## fn info
A purviews file that EXISTS but will not decode is an ERROR, never a silently empty and
therefore silently total view. The rig index already holds itself to that rule and this
follows it.

`@fae` and `fae` name the same purview here, so a caller can paste a reference straight out
of a purview's configuration without stripping the sigil.

FILTERING EXPANDS `@` references while the reported `purview` field carries the RAW
configuration - the two go through different functions on purpose, so a report shows what
was written and the filter matches what it means.
