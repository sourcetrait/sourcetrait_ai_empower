# link.rs

The link lifecycle over the codec (codec.rs) + verifiers (verify.rs). One LINK =
two entity-pinned mTLS connections to one peer, message/control + file, so file
bulk never head-of-line-blocks control (TCP is one ordered stream, so separation
is two connections, not app-mux; understood/17). Shape from the raw_tls pattern +
~/info/example/tls (client.rs/server.rs the lifecycle skeleton), with the mTLS
divergence below. Slice-D scope is the lifecycle only; what is NOT here is
deliberate (see scope at the end).

## struct RemoteLinkOptions
Explicit cert PATHS, not config: slice D is config-agnostic so link mechanics and
the `[remote.<alias>]` file layer (leg 4, understood/11) stay decoupled. self leaf
= this host's srcert `entity_grammar.pem` (the same leaf the WSS hub presents,
now client-capable since CertMigration part 1); remote pin = the peer's
`entity_<nom>.pem`. Leg 4 wires these from the channel cert_dir + per-remote
config.

## struct RemoteLinkHandle
### fn close
The cancel/timeout-join/abort ladder from the example: cancel the shared token
(both drivers observe it and run their Close/Close exchange), await each JoinHandle
under `timeout`, abort only the ones that overran. `Option<JoinHandle>` + `take`
so close is idempotent-ish and a handle is not awaited twice. ONE CancellationToken
shared by clone across both connection drivers (clone shares cancel state, so
cancelling the handle's token cancels both), not child tokens - a link cancels as
a unit.

## struct RemoteLink
### fn connect
The INITIATOR side: build the client config once, open both connections
(connect_conn per stream), and require both to report the SAME peer McpNom
(pair_check) before spawning the drivers - a mismatch means the two TCP
connections reached different hosts, which must never become one link. The
initiator controls both connections, so pairing them is trivial here; the hard
pairing is on the acceptor.

### fn accept
The ACCEPTOR side, SLICE-D SCOPE: accept connections off `listener` until both the
Message and File halves have arrived, then pair by the same-McpNom check. This
assumes the two connections belong to ONE initiator (no interleaving) - correct
for the loopback + a single link, WRONG for concurrent initiators. Leg 4
(RemoteTools) replaces this with a persistent listener + a pairing coordinator
keyed by initiator McpNom + a NuSh link registry; slice D proves the per-link
mechanics the registry will drive. The `acceptor` (TlsAcceptor) is passed in, built
by the caller via `server_config`, so leg 4 owns the listen socket's lifetime.

## the McpNom handshake
Modelled as a `Hello` variant in each language (codec.rs), exchanged over the same
FramedRead/FramedWrite before the steady loop - not a separate pre-framing codec,
which would mean two codecs per connection. The initiator sends first (its nom +
the stream role), the acceptor replies its nom. Endpoints identify by McpNom only;
McpNom -> AI_ID is the agent's own from-field inference, out of scope
(understood/17). The nom crosses as its base62 String (McpNom wraps a private
Nonce(u64), not bitcode-serializable), so link.rs takes/stores the String form and
leg 4 passes `NuSh.mcp_nom.to_string()`.

## mTLS entity-pin config (the example's divergence)
The example wires `with_no_client_auth` on BOTH sides (server-auth ONLY); we do
NOT copy that (it is the "comes up working and unauthenticated" shape for a mutual
link). Instead:
- client: `builder_with_provider(ring)` ... `.dangerous().with_custom_certificate_verifier(EntityPin)` `.with_client_auth_cert(leaf, key)` - present our leaf AND pin the server's.
- server: `builder_with_provider(ring)` `.with_client_cert_verifier(EntityPin)` `.with_single_cert(leaf, key)` - present our leaf AND require+pin the client's.
Ring provider by hand (rustls_ring_pin) so the link never depends on whatever
installed a process default and no second backend creeps in. EntityPin implements
both verifier traits (verify.rs), so the same value serves both sides. Single-leaf
chain (`load_chain` = `vec![leaf]`), like the WSS hub: entity-pin matches the leaf
DER and ignores any chain, so no intermediates are needed. `server_config` is a
pub(crate) helper the acceptor's caller uses; unused inside link.rs itself in
slice D (dead-code-allowed), consumed by the leg-4 listener + the leg-6 test.

## PIN_SERVER_NAME
rustls's ClientConfig still needs a syntactically valid ServerName to drive the
handshake even though EntityPin's verify_server_cert ignores the name (it matches
the leaf DER). A fixed `.invalid` placeholder keeps that explicit - there is no
real DNS identity to assert under entity-pin.

## the driver loop (run_initiator_conn / run_acceptor_conn)
`tokio::select!` with `biased;` and cancel FIRST, so a close beats an in-flight
read deterministically (the channel hub's close-race discipline, understood/14).
Two concrete driver fns rather than one generic over the languages: the In/Out
enums + client-vs-server TlsStream types differ per side, and abstracting "has a
Close variant" behind a trait costs more than the ~20 duplicated lines buys
(raw_tls: granularity is cheap, spend it). A stray post-handshake `Hello` is
IGNORED (the empty arm), not fatal: lenient-in-what-you-accept for an unexpected
control frame, and - load-bearing for now - it gives the loop a non-terminating
path so clippy's never_loop does not fire while Close is the only other message
(the leg-5 send/recv arms will forward-and-continue too). close_initiator /
close_acceptor mirror the example's close_frame: send Close, briefly await the
peer's reciprocal Close (CLOSE_FRAME_GRACE) so both ends see a clean shutdown
rather than a bare connection drop.

## slice-D scope (what is deliberately absent)
- NO message send/receive plumbing: the languages carry only Hello + Close, so a
  link can be opened, identified, and closed but not yet used to pass data. The
  mpsc send/recv channels + the request/response + notice variants land with
  RemoteSend (leg 5).
- NO NuSh listener, link registry, or MCP tools: leg 4 (RemoteTools).
- NO runtime exercise: connect/accept/handshake/close COMPILE and pass clippy but
  have not been run. The loopback verification (two entities under one test CA, a
  bound port, connect <-> accept) is system/in-process-integration tier and lands
  with RemoteChannelTests (leg 6), which also needs the leg-4 tools for the
  end-to-end path. This mirror + the code are the design of record until then.
