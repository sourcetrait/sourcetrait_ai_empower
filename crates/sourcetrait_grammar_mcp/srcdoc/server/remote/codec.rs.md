# codec.rs

## enum RemoteStream
Which of a link's two connections a handshake announces: Message (control) or
File. Carried in the initiator's Hello so the acceptor knows which connection it
just took, keeping file bulk off the control path (understood/17). Copy fieldless
tag; bitcode + serde derives like the languages.

## enum InitiatorToAcceptor
## enum AcceptorToInitiator
Two languages, one per direction, so the type system forbids sending a message
the wrong way (raw_tls pattern). Named initiator/acceptor, not client/server:
both ends are Grammar MCP peers and the roles are per-connection (a host is the
initiator of links it opens, the acceptor of links opened to it), so peer-neutral
names read truer than the example's client/server (settles the followup's open
naming decision). bitcode::Encode/Decode is the wire format; the serde derives
ride along for any future non-wire use. Four variants each: Hello (the opening
McpNom handshake - the initiator's also names the stream, the acceptor's just
replies its nom), Msg(MsgFrame), File(FileFrame), and Close. There is NO
request/response pair: MsgFrame carries only Deliver (no DeliverAck), because the
sender's own transport WRITE is the delivery ack - Sent/Unsent fire from it
(understood/17). DeliveryResult is gone with the ack it reported.

## const REMOTE_ZSTD_LEVEL
Every frame is zstd-compressed before framing (the_user): `zstd(bitcode(msg))`,
applied UNIFORMLY to both streams rather than gated on stream type. Obviously worth
it for file chunks, but also for the control messages, which are string-heavy
(McpNoms, and the file paths + payloads of the leg-5 send path) and compress well
(the_user). Level 3 (zstd's default) is the speed/ratio balance; a single level
keeps the codec branch-free. Tiny handshake frames pay a few bytes of zstd overhead,
accepted for the uniform rule.

## struct BitcodeCodec
A tokio-util Decoder+Encoder wrapping LengthDelimitedCodec: the length prefix frames
the stream, and each frame is zstd-compressed bitcode (encode: bitcode -> zstd ->
frame; decode reverses it). `PhantomData<T>` binds the codec to one message type, so
a `FramedRead<_, BitcodeCodec<AcceptorToInitiator>>` can only yield that language.
io::Error is what the Framed traits want. Shape from ~/info/example/tls/frame.rs (a
working reference), with the zstd layer added. The round-trip is unit-tested
(server/tests/remote.rs) since the TLS lifecycle that would exercise it end-to-end
is deferred to leg 6.
