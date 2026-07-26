# hub.rs

## const BIND
The channel is a single-host design, so "only localhost gets in" is a property of the
SOCKET rather than a policy to enforce - the kernel will not route anything else to a
127.0.0.1 listener. That is why there is no peer allow list, deny list, origin check or
per-connection CA pinning anywhere in this file.

A literal rather than `localhost` also removes the resolution ambiguity that once made a
v4 reset read as a TLS verdict during the live exercises; the leaf carries IP SANs for
this.

## const FROM_MCP
Plain `mcp`, not `mcp/<id>`: the channel is 1:1, so there is no second MCP to tell apart.

## fn open_packet
Built in one place because it has TWO emitters: the hub greets a freshly connected peer
with it, and `channel_open` re-sends it on an EXISTING channel, where a FAILING send is
what reveals a peer that has actually gone.

## fn server_config
The ring provider is passed EXPLICITLY rather than taken from the process default. rustls
defaults to aws_lc_rs while nushell already compiles against ring, so naming it keeps the
hub independent of whatever else may have installed a default - and keeps a second crypto
backend out of the dependency graph.

`with_no_client_auth` is deliberate, not an omission. The peer is authenticated by the
HANDSHAKE over the already-trusted stdio session, which is all the agent's Monitor can
support: its input is `{url, protocols}` and carries no headers, no client cert and no auth
hook.

## fn start
EPHEMERAL PORT by default. The agent always learns the endpoint from `channel_open`'s
return, so a fixed port buys nothing and costs a squatting failure mode - a stale host or
an escaped child holding it makes the next open fail with a raw `Address already in use`,
which was hit live.

The port is read back from `local_addr` rather than trusted from config: with 0 it is the
only way to know it, and with a pinned one it confirms what was actually bound.

## fn accept_loop
FIRST CONNECTION CLAIMS, via `compare_exchange` rather than a load followed by a store, so
two simultaneous connections cannot both read "unclaimed" and both win.

Dropping the shutdown SENDER counts as shutdown, which is why a torn-down channel ends this
loop even though nothing is ever sent on that oneshot.

## fn serve_peer
The winner receives `channel/Open` IMMEDIATELY, because the verification step is that the
agent SEES a real packet - there is no separate ack channel by design.

THE LOOP IS `biased;` AND THE ARM ORDER IS LOAD-BEARING. `close_locked` signals the close
AND drops the packet sender in the same breath, so the close arm and the `None` arm go
ready TOGETHER - and `tokio::select!` picks among ready arms at RANDOM unless told
otherwise. The `None` arm breaks the loop without writing a frame, so a planned close
carried our code and reason only about HALF the time and otherwise left the bare 1006 the
lanes existed to prevent. Biasing makes the precedence structural instead of probabilistic,
and states the design directly: a planned close must never wait behind queued traffic.

THE WAY IT WAS FOUND IS THE PART TO REMEMBER. This was latent from the first phase - this
file did not change in the second - and that phase's live exercise recorded ONE observed
1000 as proof the mechanism worked. One sample of a coin flip is not evidence. A
nondeterministic path needs either repetition or a structural argument; this one now has
both.

POLLING THE READ HALF IS NOT OPTIONAL: it is what lets tungstenite answer pings and observe
the peer's own close.

THE FRAME GUARD LIVES HERE, at the last point before the wire. Above the cap the client
drops the frame whole and closes the watch; exactly AT it the frame arrives missing its
first byte, which is silent corruption rather than an error. So the bound is a hard `<` and
nothing may ever be sized onto 1 MiB.

## fn refuse_peer
The close CODE and REASON both reach the client verbatim, which was verified, so a refusal
is legible rather than looking like a crash. The refusal is the design working, not
something to route around.
