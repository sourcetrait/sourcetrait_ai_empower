# channel_send.rs

## struct GrimmChannelSend
THE CHANNEL IS A NOTIFICATION LANE, NOT A SERIALIZATION LANE. `event` says "state
changed, here is a tiny summary"; the agent fetches the actual data itself. That
premise is why the signature has TWO slots rather than one: an author with
something bulky puts it in `attached`, which the host writes to the inbox and
NAMES on the wire rather than carrying. The contract is STRUCTURAL - the ergonomic
path is the next argument - so nobody has to be talked out of putting data on the
wire.

It returns the message id so an author can correlate what it sent with what the
agent later fetches.

### fn run
ORDER IS DELIBERATE THROUGHOUT, and each step is placed where it is for a reason.

The `mcp/` REFUSAL comes first, before the channel state is even consulted. A body
is not the host, so letting it stamp a model under that prefix would let it forge a
host control packet - and would destroy the property the reservation exists for,
that provenance is checkable from the path alone. This is the one place a non-host
picks a model, which is what makes it the right place to enforce it.

The PHASE is checked before any rendering, so a send that cannot happen costs a
lock and an error rather than a hash and a render.

OPEN-BUT-UNVERIFIED does not merely refuse - it TEARS THE CHANNEL DOWN. Telemetry
must not reach a peer that has not proven it owns the stdio session, and a peer
that has been emitted to unproven is not a peer we keep.

The phase is read HERE, at send time, never captured: a job outliving its eval
keeps its decl and everything it closed over, so a spawn-time snapshot would let
work started before verification emit to an unproven peer.

The id is minted BEFORE the attachment write because the filename derives from it,
which is also why the hash covers the attached CONTENT rather than its path.

CapNoCap: after the id and the caller's attachment, the rendered event is checked
against the notification cap (event_overflows, state.rs); an oversized event spills
to inbox/<id>.event.nuon and a compact pointer rides the wire in its place. The
pointer names its own spill file, so it is orthogonal to the caller's `attached` -
a big event and a caller attachment both survive, in separate files. write_inbox_file
is the generalized writer serving both.

## fn shell_error
`GenericError` renders its TITLE through Display, and the title is all the eval
envelope's `message` carries - so a bare command name there reaches the agent with
the cause stripped off. That is the opaque-error failure this crate already knows
well, and it recurs because the label looks like the natural home for detail.

## fn stop_offender
TWO LEVERS, because the offender has two shapes and neither lever reaches both.

`Signals::trigger` reaches a spam loop whatever it runs under: such a loop invokes
a decl over and over, so it polls at every one of those boundaries and bails.

But a job that OUTLIVED the eval that spawned it has no `in_flight` entry
left - `InFlightCleanup` removed that the moment the dispatch returned - and the
jobs table is the only place it is still reachable. A spawned closure reads its OWN
JobId here, so `current_job` names the actual offender rather than its parent.

PROVEN LIVE: a detached `job spawn` whose loop CAUGHT every throw and had 196
iterations left still stopped at the fifth send, and the jobs table went empty.
That is the case the throw alone cannot cover, and the table emptying is the half
`Signals` could never do. The `action` string records which branch ran, which is
also how it is verified.

Only the PROCESSES of a killed job can fail to die; the table entry goes either
way, and an external the job left behind is the tree-kill machinery's to reap.
