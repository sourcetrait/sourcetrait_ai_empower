# verify.rs

## struct EntityPin
The security core of the remote transport. Trust is ENTITY-PIN, not CA-chain: a
peer is accepted iff its presented end-entity leaf DER is byte-equal to one
configured certificate. Deliberately independent of the system trust store + CA
validation the WSS channel uses (understood/14) - a remote link authenticates a
SPECIFIC peer host (its srcert-minted entity_<nom>.pem), not "anyone our CA
signed".

One EntityPin implements BOTH rustls verifier traits because each end pins the
SAME remote leaf regardless of role: as the connecting client it is the
ServerCertVerifier (checks the remote's server cert), as the accepting server it
is the ClientCertVerifier (checks the remote's client cert). mTLS runs both
checks, so both traits are needed and both pin identically.

### the signature methods are not optional
The pin check is necessary but not sufficient: matching the leaf DER proves it is
the right certificate, but the handshake SIGNATURE proves the peer holds the
matching private key (a public leaf alone can be replayed by anyone). So
verify_tls12/13_signature delegate to the ring provider's
signature_verification_algorithms rather than being stubbed - stubbing them is the
classic "comes up working and unauthenticated" hole. supported_verify_schemes
reports the provider's real schemes so rustls negotiates one the delegation can
check.

### root_hint_subjects empty
The hint tells a client which CAs the server trusts - meaningless under entity-pin
(there is no CA to hint). Empty tells the client to send whatever leaf it has,
which the pin then checks.

provider = ring, matching the crate's other TLS (rustls_ring_pin); no second
backend. The unit test (server/tests/remote.rs) covers the pin decision over raw
DER blobs; the full handshake is system-tier (RemoteChannelTests leg).

## struct UnionPin
The acceptor's verifier (leg 4c): a peer is accepted iff its client leaf DER
byte-matches ANY of a configured set - the UNION over every `[remote.<alias>]`
remote_public_key_file. Same entity-pin trust as EntityPin, widened from one leaf
to a set, because the acceptor does not know which peer is connecting until after
the TLS handshake and so must trust every configured peer at once. A
ClientCertVerifier ONLY: the acceptor is always the TLS server, so unlike EntityPin
(dual-role for the symmetric single-link path) it never verifies a server cert. The
signature-delegation and empty root_hint_subjects are identical to EntityPin and
load-bearing for the same reason (a matched leaf without a proven private key is a
replayed public cert). An empty set trusts nobody - a listener configured with no
loadable peer pins rejects every inbound link, which is logged at startup. Unit-
tested (server/tests/remote.rs) over raw DER; the full handshake is leg-6 system-tier.
