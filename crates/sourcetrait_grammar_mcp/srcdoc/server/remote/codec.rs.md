# codec.rs

## enum InitiatorToAcceptor
## enum AcceptorToInitiator
Two languages, one per direction, so the type system forbids sending a message
the wrong way (raw_tls pattern). Named initiator/acceptor, not client/server:
both ends are Grammar MCP peers and the roles are per-connection (a host is the
initiator of links it opens, the acceptor of links opened to it), so peer-neutral
names read truer than the example's client/server (settles the followup's open
naming decision). bitcode::Encode/Decode is the wire format; the serde derives
ride along for any future non-wire use. Minimal today (Close only); the
request/response + notice variants land with the send path (RemoteSend leg).

## struct BitcodeCodec
A tokio-util Decoder+Encoder wrapping LengthDelimitedCodec: the length prefix
frames the stream, bitcode is the payload. `PhantomData<T>` binds the codec to one
message type, so a `FramedRead<_, BitcodeCodec<AcceptorToInitiator>>` can only
yield that language. io::Error is what the Framed traits want. Shape copied from
~/info/example/tls/frame.rs, a working reference.
