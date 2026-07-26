# state.rs

## const MCP_RESERVED_PREFIX
Every model the HOST stamps lives beneath it. The value of a reservation is that it makes
provenance a MECHANICAL check: anything claiming `mcp/` from a source that is not the host
can be rejected without interpreting it. Enforced today at the one place a non-host picks a
model, and available to a future peer surface on the same terms.

## const MAX_FRAME_BYTES
A hard `<`, never a `<=` against 1 MiB. A frame landing EXACTLY on the cap arrives missing
its first byte - not an error and not a dropped event, but a malformed record that fails
`from nuon` at the reader with nothing upstream to blame. Above the cap the frame is dropped
whole and the watch closes.

## enum ChannelPhase
READ AT SEND TIME, NEVER CAPTURED. A job outliving its eval keeps its decl and everything it
closed over, so a spawn-time snapshot would let work started before verification emit to an
unproven peer - exactly what the gating forbids.

Channels are OPTIONAL and start lazily on the first `channel_open()`, which is why `Closed`
is an ordinary state and an emit there is a plain catchable error rather than a fault.

## type CloseSignal
The completion half exists for SHUTDOWN. The frame is written by the hub task
asynchronously, so a caller that exits the process immediately after asking for a close
races that write and leaves the peer with exactly the bare 1006 the explicit close exists to
prevent. Awaiting an ack makes the ordering deterministic instead of a sleep long enough to
usually work.

## enum SpamVerdict
`notify` on `Stop` is true only on the FIRST crossing of an episode: the refusal has to
persist for every later send, but announcing it every time would make the report about spam
into spam itself.

## struct OriginCounter
Both flags reset when the window empties and the origin is EVICTED, so a producer that goes
quiet and later misbehaves again is a fresh episode rather than one that never ended.

## struct ChannelInner

### field counters
Bounded by EVICTION rather than by a cap: an origin whose window empties is dropped, so this
holds only currently-emitting work rather than one entry per origin for the host's whole
life.

### field close
THE CLOSE SIGNAL DOES NOT SHARE THE PACKET QUEUE. A close queued behind traffic could arrive
late or not at all, and without an explicit close frame the agent sees a bare 1006 -
indistinguishable from a crash.

### field verify_cancel
Holding the sender is what keeps the timer armed and DROPPING it is the cancel, so
verification, a close and a re-arm all stand the timer down. That is also why re-opening
cannot leave two timers racing one channel.

### field inbox
A DELIBERATE `channel_close()` takes it and prunes it; every OTHER teardown leaves it. The
shutdown close, the verify-timer expiry and the unverified-emit teardown are all
HOST-initiated, so an attachment stays readable by nonce when the channel went away without
the caller asking for it.

## enum ChannelSendError
The variants are deliberately DISTINGUISHABLE: a background consumer must be able to tell
"stop working, the channel is gone" from "this one send failed", or it will either exit on a
blip or spin against a dead channel.

## struct ChannelHandle
Guarded by a **std** Mutex rather than an async one on purpose. The emit path runs on the
eval thread inside a synchronous nu `Command::run`, and `UnboundedSender::send` is itself
sync, so the whole path stays lock-cheap and needs no runtime handle.

### fn new
Taking the starting policy rather than reading CONFIG is what makes the handle constructible
without a process-global, which is the only reason the state machine and the counter can be
unit-tested at all.

### fn take_inbox
CLEARING is what makes it safe twice over: a second close cannot prune a directory a later
open recreated.

### fn mark_verified
VERIFICATION REQUIRES A CLAIM. The handshake asserts the agent owns THE CLAIMING connection,
so with no claim there is nothing to own. It also closes a real hole - a
verified-but-unclaimed channel would accept emits into a queue nothing drains, because the
receiver is still parked in the accept loop waiting for a peer.

### fn close_if_unverified
The phase is re-read HERE, under the lock, because verification can land between the timer's
sleep elapsing and its task being scheduled; a bare `close` would then tear down a channel
that had just proven itself.

### fn send_control
BYPASSES the gate, and `emit` does not. That split is load-bearing rather than a
convenience: the gate keeps TELEMETRY off an unverified peer, but the handshake packet is
the thing the peer verifies ITSELF by seeing, so it necessarily precedes verification.

### fn record_send_at
Counting keys on the ORIGIN, which is host-stamped, so a body cannot spread its traffic
across identities to stay under the rate.

## static CHANNEL
A process-global rather than a threaded parameter. The channel is one-per-process by design,
exactly like CONFIG, and the alternative is passing an `Arc<ChannelHandle>` through NuSh,
dispatch, the eval entry points and `register_nuapi` - five signatures on the hot eval
path - for a value that can never vary per eval. `NuSh` reads the same handle rather than
owning its own, so the two can never diverge.

## fn close_locked
The teardown itself, with the lock already held, so the two entry points that decide WHETHER
to close cannot drift about WHAT closing does.

Dropping the shutdown sender signals the hub task even though nothing is ever received on
it.

## struct MsgId
Distinct from `Nonce` in NAME rather than in shape: nonces name EVALS throughout this crate,
so reusing the word on the wire would confuse two different things.

## fn mint_msg_id
The ATTACHED CONTENT is hashed, never its path - the path is derived FROM the id, so hashing
it would be circular. The generator mixes in a counter and a timestamp, so two byte-identical
packets still get distinct ids.

## fn render_nuon
Rendering from a Value is deterministic, which is what lets the id hash cover the rendered
form.

## fn escape_line
`to nuon` renders compactly but does NOT escape a newline inside a string value, and the
client BATCHES frames arriving close together into one event joined by newlines. So a
literal newline in a packet is indistinguishable from a batch boundary and a reader would
see MORE records than were sent.

Safe because the only raw newlines a compact render can carry are inside double-quoted
strings, where `\n` and `\r` ARE the escapes nushell reads back - the line still parses to
the original value.
