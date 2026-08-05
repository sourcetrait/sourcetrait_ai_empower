# link.rs

The link lifecycle over the codec (codec.rs) + verifier (verify.rs). One LINK = two
known-public-key mTLS connections to one peer, message/control + file, so file bulk
never head-of-line-blocks control (TCP is one ordered stream, so separation is two
connections, not app-mux; understood/17). Shape from the raw_tls pattern +
~/info/example/tls, with the mTLS divergence below. Opened ON DEMAND after each
agent's Channel is verified; NO run_server auto-listener (the retired leg-4c
config-driven acceptor + its pairing coordinator + union_server_config are gone).

## fn open_remote_blocking (open_connector / open_listener)
Blocks on the link's immediate networking result so remote_channel_open returns
synchronously; only a listener's peer-wait stays async. Void-then-async was the wrong
shape (RemoteFirstBlood/ConnectionWoes): binding is immediate, so a bind clash from a
redundant listener open surfaced late as a confusing async Disconnected{error} instead
of a synchronous "address already in use" (the queen's first-blood double-open).

- Connector (open_connector): await connect_link under CONNECT_TIMEOUT (20s). On success
  register the link and spawn only a teardown watcher (await both joins -> deregister ->
  Disconnected); there is no Connected for the open attempt - the synchronous success is
  the notice. A connect error/timeout returns Err, surfaced as remote::open_failed. A
  redundant connector open fails its connect synchronously, or is caught by the already-open
  guard once the first link registers.
- Listener (open_listener): await bind_listener (bind + build the acceptor, the sync half)
  and return the bind result synchronously (bound, or the bind error as remote::open_failed).
  Then spawn the async accept-serve: accept_and_pair one peer's two connections, register the
  link, emit Connected {remote, mcp_nom}, await both joins, deregister, emit Disconnected. An
  accept failure after a good bind emits Disconnected {remote, error} (post-bind, so async).

The only lifecycle events removed are Connected/Disconnected for a connector's OPEN ATTEMPT
(now the synchronous return). Disconnected is retained for any established-link teardown,
both roles (the_user): once told the link is up - Connected for a listener's peer, or the
sync open-success for a connector - a later drop still emits Disconnected so a long-running
consumer learns the live link died. deregister_if_ours drops the registry entry only if it
is still this task's link (a re-open may have replaced it under the same alias).
emit_open_failed (Disconnected {remote, error}, mcp_nom absent) now serves only the
listener's post-bind accept failure; the connector and bind failures return synchronously.

`remote` (the opened alias) is always present on a Connected/Disconnected packet, so a
consumer keys on it and branches on field presence (attached?, understood/14): mcp_nom
present iff the link established, error present iff it failed - absent when not, never null
(the_user).

## static BOUND_LISTENERS / fn unlisten
The bound-but-unpaired listener registry (RemoteFirstBlood/RemoteChannelListeners),
parallel to REMOTE_LINKS. open_listener registers the alias -> bind address the moment the
bind succeeds - the clean registration point ConnectionWoes's synchronous bind created - so
remote_channels can surface a listener that has bound but not yet paired, which was
invisible before (remote_links holds only established links, so a bound listener read as
empty, indistinguishable from down). On pairing the accept-serve task registers the
established link THEN unlistens (briefly in both sets, never neither); on an accept failure
it unlistens then emits Disconnected. unlisten is unconditional (no peer-nom guard like
deregister_if_ours): the synchronous bind serializes opens on one address, so at most one
accept-serve task owns an alias here.

## fn find_link_send / fn safe_dest
find_link_send matches on the link's remote_mcp_nom (the id an agent sends to), holds
the registry lock only for the lookup + the synchronous mpsc pushes. safe_dest is the
one guard for a dest (relative, no .., all Normal components), applied sender-side
(clean error) AND receiver-side (defence against a buggy peer).

## struct RemoteLinkHandle
### fn enqueue_send
REGISTERS the send in the SendTracker (its dests + the message) BEFORE any push, so
no write outcome can arrive for an unknown id; a mid-push failure means the driver is
gone (link down at enqueue) - forget the registration and return a SYNCHRONOUS error
(no id, no async notice, since nothing left the host). push_frames is the free helper
doing the pushes; splitting it out keeps the register/forget bracket readable.

## struct SendTracker / struct PendingSend
The aggregate write-outcome half - the corrected design's core. A send is Sent only
when the Deliver AND every file dest's last chunk have written OK across BOTH
connections (the whole transmission), Unsent on the first write error or a link
teardown mid-send. This is NOT a peer round-trip: the transport WRITE is the ack
(TCP/TLS performs it), so there is no DeliverAck and no receipt timeout. Per-id map:
each driver reports its writes (dest_written on a last-chunk write, message_written on
the Deliver write), completeness is checked under the mutex, and whichever write
finishes the set fires Sent exactly once (map remove). failed() is idempotent per id
and cancels the shared token so the other connection winds down; flush_unsent drains
the map on each driver's exit, idempotent, so a teardown reports every still-pending
send as Unsent without double-firing. Files ride the file connection and the Deliver
the message connection, so aggregating "the whole send wrote" needs this shared
per-link tracker rather than a single write site.

## the receive side (RecvContext, DeliverSlot, try_finalize, relay_to_channel)
A delivered message becomes a NORMAL Channel packet (relay, NEVER "injection"). The
slot-map per id coordinates the two connections: the Deliver sets model+event+the
expected dest set, each file's last chunk marks a dest done, try_finalize fires ONCE
(map remove) when the Deliver is in AND every dest has landed -> relay via emit
(verification-gated). The receiver sends NOTHING back - its Sent already fired from
the sender's write, and a relay failure is logged locally, not surfaced remotely. The
slot-map is what makes the files EXIST on disk before the agent observes the emission
(understood/17). land_file_chunk appends under <inbox>/<peer_nom>/<id>/<dest> - the
same inbox the Channel's own attachments use, so a relayed packet's `attached` ref
resolves for the local agent.

rewrite_spill_pointer (CapNoCap): a sender spills an oversized event to the reserved
EVENT_SPILL_DEST file with a dest-relative pointer; relay_to_channel rewrites that path to
<peer_nom>/<id>/<dest> before emitting, so the agent resolves it as
<inbox>/<spilled_event_path> uniformly with a local spill. A no-op for a normal event or a
path already carrying a `/`.

## the McpNom handshake
A Hello variant in each language (codec.rs), exchanged over the same Framed
read/write before the steady loop. The initiator sends first (its nom + the stream
role), the acceptor replies its nom. pair_check requires both connections of a link to
report the SAME peer nom - a mismatch means the two TCP connections reached different
hosts, which must never become one link. The nom crosses as its base62 String (McpNom
wraps a private Nonce(u64), not bitcode-serializable).

## the driver loops
Two concrete driver fns per side (message + file), `tokio::select!` with `biased;` and
cancel FIRST, so a close beats an in-flight read deterministically (the channel hub's
close-race discipline, understood/14). The In/Out enum + client-vs-server TlsStream
types differ per side, so abstracting them behind a trait costs more than the
duplicated lines buys (raw_tls: granularity is cheap). close_initiator /
close_acceptor send Close + briefly await the reciprocal (CLOSE_FRAME_GRACE 2s) so
both ends see a clean shutdown rather than a bare drop.

## known-public-key mTLS config (the example's divergence)
The example wires with_no_client_auth on BOTH sides (server-auth ONLY); we do NOT copy
that (it is the "comes up working and unauthenticated" shape for a mutual link).
Instead: client = builder_with_provider(ring) .dangerous()
.with_custom_certificate_verifier(PublicKeyVerifier) .with_client_auth_cert(our public
key, our private key) - present ours AND match the server's; server =
builder_with_provider(ring) .with_client_cert_verifier(PublicKeyVerifier)
.with_single_cert(ours) - present ours AND require+match the client's. Ring provider by
hand (rustls_ring_pin) so the link never depends on a process default and no second
backend creeps in. PublicKeyVerifier implements both verifier traits (verify.rs), so
one value serves both sides. Single-leaf chain (load_chain = vec![the public key]): the
match uses the leaf DER and ignores any chain. PLACEHOLDER_SERVER_NAME is a fixed
.invalid placeholder - rustls needs a syntactically valid ServerName even though the
match ignores it (there is no real DNS identity under known-public-key).

## runtime proof
The TLS lifecycle IS runtime-exercised: RemoteChannelTests (sourcetrait_grammar_tests
tests/remote_channel.rs, harness src/remote.rs) links two real hosts over mTLS through a
reusable simulated rmcp consumer and runs an A->B->A echo - connect/accept/handshake, a
real send + relay, and the Connected/Sent/Disconnected notice shapes - all green. The
unit tests (codec round-trip, the verifier match) stand alongside it.
