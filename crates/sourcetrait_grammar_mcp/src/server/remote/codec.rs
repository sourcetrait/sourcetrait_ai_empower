#![allow(dead_code)]
//! The remote-link wire: zstd-compressed bitcode in a length-delimited frame.
use crate::*;

/// The zstd level applied to every remote frame before it hits the wire.
const REMOTE_ZSTD_LEVEL: i32 = 3;

/// Which of a link's two connections a handshake announces.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum RemoteStream {
    /// The message/control connection.
    Message,
    /// The file-transfer connection.
    File,
}

/// The result a receiver reports for one delivery: accepted, or refused with a
/// reason (a delivery whose message it could not inject or whose files it could
/// not land).
#[derive(
    Debug, Clone, PartialEq, Eq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum DeliveryResult {
    /// Injected into the receiver's Channel; files (if any) landed.
    Accepted,
    /// Refused; `kind`/`message` say why (channel closed, event unparseable, ...).
    Refused { kind: String, message: String },
}

/// A message-connection frame, peer-neutral (both ends send both variants). The
/// request/response pair is Deliver -> DeliverAck, one answer per send.
#[derive(
    Debug, Clone, PartialEq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum MsgFrame {
    /// A message to deliver: the sender's message id, the model + event NUON the
    /// receiver injects, and the dest names it must receive on the file
    /// connection before injecting (empty for a message with no files).
    Deliver {
        id: String,
        model: String,
        event_nuon: String,
        files: Vec<String>,
    },
    /// The one answer per Deliver, correlated by `id`.
    DeliverAck { id: String, result: DeliveryResult },
}

/// A file-connection frame: one ordered chunk of one dest file for a delivery.
#[derive(
    Debug, Clone, PartialEq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum FileFrame {
    Chunk {
        id: String,
        dest: String,
        seq: u32,
        bytes: Vec<u8>,
        last: bool,
    },
}

/// Messages the link INITIATOR sends to the ACCEPTOR. A separate language per
/// direction so a mis-send is a compile error, not a runtime surprise; the
/// app-level frames (`Msg`/`File`) are peer-neutral and shared both ways.
#[derive(
    Debug, Clone, PartialEq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum InitiatorToAcceptor {
    /// Opening handshake: the initiator's McpNom and which stream this
    /// connection carries.
    Hello { mcp_nom: String, stream: RemoteStream },
    /// A message-connection frame (rides the Message connection).
    Msg(MsgFrame),
    /// A file-connection frame (rides the File connection).
    File(FileFrame),
    /// Transport-level close; the peer answers with its own `Close`.
    Close,
}

/// Messages the link ACCEPTOR sends to the INITIATOR.
#[derive(
    Debug, Clone, PartialEq, ser::Serialize, ser::Deserialize, bitcode::Encode, bitcode::Decode,
)]
pub(crate) enum AcceptorToInitiator {
    /// Handshake reply: the acceptor's McpNom.
    Hello { mcp_nom: String },
    Msg(MsgFrame),
    File(FileFrame),
    Close,
}

/// A tokio-util codec: a zstd-compressed bitcode `T` in a length-delimited frame.
pub(crate) struct BitcodeCodec<T> {
    frames: tku::LengthDelimitedCodec,
    _marker: PhantomData<T>,
}

impl<T> BitcodeCodec<T> {
    pub(crate) fn new() -> Self {
        Self {
            frames: tku::LengthDelimitedCodec::new(),
            _marker: PhantomData,
        }
    }
}

impl<T> tku::Decoder for BitcodeCodec<T>
where
    T: for<'a> bitcode::Decode<'a>,
{
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut tku::BytesMut) -> Result<Option<T>, io::Error> {
        let Some(frame) = self.frames.decode(src)? else {
            return Ok(None);
        };
        let bytes = zstd::decode_all(frame.as_ref()).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("remote frame: zstd decode failed: {e}"),
            )
        })?;
        bitcode::decode(&bytes).map(Some).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "remote frame: bitcode decode failed")
        })
    }
}

impl<T> tku::Encoder<T> for BitcodeCodec<T>
where
    T: bitcode::Encode,
{
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut tku::BytesMut) -> Result<(), io::Error> {
        let bytes = bitcode::encode(&item);
        let compressed = zstd::encode_all(bytes.as_slice(), REMOTE_ZSTD_LEVEL)
            .map_err(|e| io::Error::other(format!("remote frame: zstd encode failed: {e}")))?;
        self.frames.encode(compressed.into(), dst)
    }
}
