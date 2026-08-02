# emergency.rs

CLASSIFY-FIRST. The watchdog DETECTS and CLASSIFIES resource trouble into an
`Emergency` and pushes it onto an internal MPSC; one responder consumes. Detection is
decoupled from response, and internal state is acted on INTERNALLY - never by relying on
a transmit-out succeeding.

RECOVERY IS DEFERRED: no restart, no targeted recovery. The log keeps gathering data on
which conditions actually fire before any recovery is designed. NOTICE is not deferred,
though - a warning that only reaches a log cannot do the job the Warning family exists for.

## type EmergencyTx
Unbounded so the watchdog never blocks or back-pressures. It must stay schedulable even
under total eval saturation, and the volume is low because emission is edge-triggered.

## struct HungEngineThreadEmergency
The pathological residual: an eval whose `Signals` was triggered but which never stops,
because its nu code is stuck where it never polls - a pure-Rust hot loop inside a
builtin, or a bare blocking syscall. Cooperative cancel cannot reach it and you cannot
SIGKILL a thread.

Lane-tagged with a FIELD rather than split into two look-alike variants. A stateless one
also holds a concurrency permit, so enough of them saturate run(); the interact one
deadlocks the single serial lane. `pool_held` and `pool_cap` snapshot that pressure at
detection.

An external child or plugin stuck the same way is NOT this condition - killing that
process unblocks the waiting nu thread.

## struct CpuWarningEmergency
THE WARNING FAMILY EXISTS TO GIVE NOTICE BEFORE THE SYSTEM'S OWN ERROR ARRIVES. The
system supplies the error when a level is really exceeded - VRAM ends in an OOM, CPU
simply slows until someone notices, a full filesystem returns ENOSPC. These are the
notice that comes first, so the agent can act while it still has room.

HEAVY LOAD IS NORMAL and how to react is the AGENT's call: the host never reacts to any
of them. That is the whole difference between this family and `Critical` or the spam
error, which are for things we know must not happen.

CPU here can exceed 100 on a multi-core box, since it sums all threads. It is ambiguous
alone - a legitimate heavy transform looks identical to a runaway - which is why the
suffix says Warning and nothing triggers on it.

## struct VramWarningEmergency
Attribution to our own eval children is a later refinement; this reads the whole card.

## struct ChannelSpamWarningEmergency
NOT the Warning family despite the shared suffix. Spam implies a BUG rather than load,
so it is the one thing we stop.

Fired ONCE per origin, so the report about spam never becomes spam itself. It carries no
payload example deliberately - the agent is already being spammed by that, and can
investigate the cause itself.

`origin`, NEVER `from`. The packet envelope already carries a `from` - the SENDER, which
for anything here is the host - so an event field of the same name would put two
different meanings under one word in a single record. This is invisible in
`emergency.nuonl`, which has no envelope, and appears only on the wire; a unit test
asserts no variant carries a `from` event field.

## struct ChannelSpamErrorEmergency
`action` records what the host actually DID, because the lever differs between a
foreground eval and a job that outlived its own.

## struct CriticalEmergency
100% wrong, and the guaranteed-correct response is an MCP restart - the
restart-of-last-resort, which is INTENDED host teardown and the legitimate counterpart
to the shadowed body `exit`. The response is deferred; for now Critical only logs.

## fn Emergency::model
`mcp/` IS A RESERVATION. Every host-originated model lives beneath it, which is what
lets a model path from a FOREIGN source be checked mechanically - anything claiming
`mcp/` that is not the host is not entitled to it. Enforced today at the one place a
non-host picks a model, and available to a future peer surface on the same terms.

A unit test asserts EVERY variant is under the reservation and that the models are
distinct. One variant escaping would turn a mechanical provenance check back into a
judgement.

## fn Emergency::event_record
SHARED by the log line and the channel packet, so the durable record and the
notification can never disagree about what a condition reported. That sharing is also
what made the `from` collision above easy to get wrong.

## fn Emergency::to_nuon_line
Every field is a flat scalar - int, float, or a string with no embedded newlines - so
the record never spans lines. That is how this file sidesteps the newline hazard the
`grimm dbg` lane has to escape for: an API taking arbitrary agent data cannot make the
same promise.

## fn emergency_log_path
`mcp_nom` namespaces it per PROCESS because the cache root is only
`(id, namespace)`-scoped, so two concurrent hosts on one namespace would otherwise
clobber each other's log.

## fn append_line
Open-append-close per line: low volume, and robust against an external truncation
between writes. Nothing is buffered to lose.

## fn announce
Through `emit`, so it is VERIFICATION-GATED like any other telemetry - an unproven peer
must not receive host state. A closed or unverified channel silently DROPS the
announcement, which is correct rather than an error: channels are optional and the log
already holds the record.

No rate limiting is applied or needed. The `LevelGate` upstream already bounds each
condition to roughly one report per episode.

## fn spawn_emergency_responder
The log is written FIRST and unconditionally, so the durable record never depends on a
channel being open. The task ends when every producer drops the sender.
