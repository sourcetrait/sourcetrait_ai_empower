# common.rs

## struct RunParams
Shared by run AND interact. The only differences between the two tools are the substrate
and the template, so a second identical params type would be duplication with a drift
risk.

Every object-shaped field is `mcp::JsonObject` rather than `serde_json::Value`, and that is
not a style choice: schemars renders `Value` as the JSON Schema `true` keyword, which
Claude Code's client REJECTS, while `JsonObject` renders `{"type": "object"}`. The same
gotcha applies to every envelope's `result` field.

A `JsonObject` at the top level also sidesteps the recursive-outputSchema `$defs`/`$ref`
watch-item, since there is no JsonSchema-derived recursive enum on the schema params.

`timeout_ms` has NO upper cap - the agent picks. None means `DEFAULT_TIMEOUT_MS`.

## struct CachedRunBody
Carries the CONVERTED nu positional-type strings, not the structured JSON schemas the agent
submitted. Conversion already happened once, so rerun feeds the converted types straight to
the template with no re-conversion and no re-lint.

Per-call args are deliberately NOT cached; they are supplied fresh at rerun time, which is
the whole point of the handle.

## fn write_run_body
PRE-DISPATCH and for EVERY run-family eval - success, error, or timeout alike. That is what
leaves a timed-out run rerunnable: its error envelope carries the nonce, and
`rerun(nonce, same_args, timeout_ms: bigger)` recovers it.

Non-fatal by design: a write failure logs and the call proceeds, so the run still returns
its result and nonce and only a later rerun would miss the body.

## struct NuSh
One instance per process. The rmcp `#[tool_handler]` dispatches off `self.tool_router`,
which is assembled deny-filtered at construction.

### field channel
The process-wide handle, NOT a fresh one. An eval reaches the channel through the same
global, so a per-NuSh handle would diverge from what bodies actually emit on.

### field channel_open_lock
ASYNC because it is held across an await; the channel's own state lock is a std Mutex
precisely because the emit path must never need a runtime handle. The two locks differ in
kind for opposite reasons, which is worth not "simplifying".

## struct InFlightEntry
`processes()` snapshots this and `kill(nonce)` reads it. The visibility caveat is
unchanged and deliberate: `processes()` shows ALL in-flight work on the host, so subagents
see each other's calls and the primary's.

## fn NuSh::new
`nu::CRYPTO_PROVIDER.default()` installs nushell's TLS provider once so the `http` family
works. Nushell reads its OWN OnceLock here, never rustls's process-global, which is why
this call rather than a rustls one.

## fn dispatch_pooled
THE HANDLER MINTS THE NONCE BEFORE SOURCE SYNTHESIS, so the builder can embed it as
`$env.NONCE` and the dispatch then reuses the same nonce - which is what makes the ambient
value equal the envelope's.

`refresh_base_if_stale` runs before taking a clone, so this eval runs against current
plugin decls rather than whatever the base held when it was built.

The tracker is set as the engine's `background_thread_job` BEFORE the eval starts, because
nushell registers an external's pid at spawn time and a tracker attached later would miss
it.

ON TIMEOUT the order matters: trigger cancel so the abandoned thread bails at nushell's
next check point and releases its permit, THEN tree-kill the external process tree, THEN
register for hang watching. A pure-Rust hung eval cannot be reached by the first step -
that is the accepted residual - and the registration is what turns it into a reported
condition rather than a silent leak.

## fn dispatch_interact
The engine handle is cloned OUT of the guard rather than held across the eval, because the
engine thread serializes calls itself - holding the guard would serialize them twice and
block an unrelated caller for the duration.

## struct InFlightCleanup
Drop SPAWNS an async removal task because `Drop` cannot await, and the map is behind an
async mutex. The consequence worth knowing: removal is not synchronous with the dispatch
returning, so a `processes()` racing a completion can still see the finished entry
briefly.

## fn teardown_all_in_flight
Called after the MCP service stops - the client closed stdin - and from the signal
handlers, so a disconnect or a signalled death mid-eval never leaks a process tree.

## fn sweep_env_jobs
THE HALF `in_flight` STRUCTURALLY CANNOT COVER. A `job spawn` that outlives its eval has
had its registry entry removed by `InFlightCleanup` the moment that dispatch returned,
while `env_jobs` still holds it - and its external child is exactly the one that would
otherwise keep running as the box user with its supervising thread already gone.

The pids are collected BEFORE `kill_all`, for two reasons: that call clears the table and
takes the tracked pid sets with it, and nushell's own kill reaches only the DIRECT child,
so the /proc descendant walk in `tree_kill` is what gets a grandchild.
