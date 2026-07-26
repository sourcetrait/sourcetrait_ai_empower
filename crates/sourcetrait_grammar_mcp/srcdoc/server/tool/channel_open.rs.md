# channel_open.rs

## const VERIFY_WINDOW
A CODE CONSTANT, not a config option: it is a property of the handshake rather than of a
deployment.

## const CLOSE_UNVERIFIED
A distinct code because close codes and reasons reach the agent VERBATIM, so the three
teardowns - refused 1013, planned 1000, unverified 1008 - stay tellable apart.

## fn ensure_inbox
Handing the path to the CHANNEL is what lets the emit path write an attachment without
knowing anything about the namespace.

## fn resend_open_packet
A FAILING SEND IS THE ONLY WAY to learn the connection has gone. Nothing else reports a peer
that simply went away, which is why an `existing` open re-greets rather than trusting its own
state.

## fn arm_verify_timer
Expiry tears the hub down; verification or a close DROPS the sender, which stands this task
down instead. That is why cancellation is a drop rather than a message - it cannot be missed.

## fn channel_open
The decide-then-start sequence spans an await, so it runs under ONE guard: two concurrent
calls would otherwise both read Closed and both bind a hub.

`existing` IS THE NORMAL CASE, not a fault, and reading it as one is expensive. The host
OUTLIVES an agent's context - it keeps running across a `/clear`, and so does a persistent
Monitor - so a channel the agent has no memory of opening is usually still open and still
claimed. THE RE-SEND IS THE LIVENESS TEST: a call that RETURNS has already proven the peer,
and only `channel::peer_gone` says otherwise. So `peer_gone` is the ONLY signal that calls
for close-then-reopen; tearing down on `existing` destroys a working channel and the Monitor
draining it. The handshake exists because the agent loses its MEMORY, not its CONNECTION.

The re-send is guarded on `claimed` because an UNCLAIMED hub has no connection to test and
greets the next Monitor itself - an extra packet there would only queue a duplicate.

The timer is armed on BOTH paths: an existing channel has to re-prove its peer too, and a
fresh arm stands the previous timer down.
