//! Unit tests for the remote-link known-public-key verifier.

use crate::rv;
use crate::server::remote::verify::PublicKeyVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::server::danger::ClientCertVerifier;

/// A leaf byte-identical to the known public key is accepted; any other leaf is
/// rejected - both as a server cert (client-side) and a client cert (server-side).
#[test]
fn public_key_verifier_accepts_only_the_matching_leaf() {
    let key_bytes = vec![1u8, 2, 3, 4, 5];
    let other_bytes = vec![9u8, 9, 9];
    let known = rv::CertificateDer::from(key_bytes.clone());
    let verifier = PublicKeyVerifier::new(known);

    let same = rv::CertificateDer::from(key_bytes);
    let different = rv::CertificateDer::from(other_bytes);
    let now = rustls::pki_types::UnixTime::now();
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();

    // Client-side: verifying the remote's SERVER cert.
    assert!(ServerCertVerifier::verify_server_cert(&verifier, &same, &[], &name, &[], now).is_ok());
    assert!(
        ServerCertVerifier::verify_server_cert(&verifier, &different, &[], &name, &[], now).is_err()
    );

    // Server-side: verifying the remote's CLIENT cert.
    assert!(ClientCertVerifier::verify_client_cert(&verifier, &same, &[], now).is_ok());
    assert!(ClientCertVerifier::verify_client_cert(&verifier, &different, &[], now).is_err());
}

/// The codec round-trips a message through zstd + bitcode: what encode writes,
/// decode reads back unchanged.
#[test]
fn bitcode_codec_round_trips_a_message_through_zstd() {
    use crate::tku::{BytesMut, Decoder, Encoder};
    use crate::{BitcodeCodec, InitiatorToAcceptor, RemoteStream};

    let msg = InitiatorToAcceptor::Hello {
        mcp_nom: "abc123XYZ".to_string(),
        stream: RemoteStream::File,
    };
    let mut dst = BytesMut::new();
    let mut codec: BitcodeCodec<InitiatorToAcceptor> = BitcodeCodec::new();
    codec.encode(msg.clone(), &mut dst).expect("encode");
    let got = codec.decode(&mut dst).expect("decode ok").expect("a full frame");
    assert_eq!(got, msg, "encode -> zstd -> frame -> decode must round-trip");
}

/// The send frames round-trip through both wire enums: a Deliver and a FileChunk
/// carrying real bytes (a live zstd payload).
#[test]
fn bitcode_codec_round_trips_the_send_frames() {
    use crate::tku::{BytesMut, Decoder, Encoder};
    use crate::{AcceptorToInitiator, BitcodeCodec, FileFrame, InitiatorToAcceptor, MsgFrame};

    fn roundtrip_init(msg: InitiatorToAcceptor) {
        let mut dst = BytesMut::new();
        let mut codec: BitcodeCodec<InitiatorToAcceptor> = BitcodeCodec::new();
        codec.encode(msg.clone(), &mut dst).expect("encode");
        let got = codec.decode(&mut dst).expect("decode ok").expect("a full frame");
        assert_eq!(got, msg);
    }
    fn roundtrip_acc(msg: AcceptorToInitiator) {
        let mut dst = BytesMut::new();
        let mut codec: BitcodeCodec<AcceptorToInitiator> = BitcodeCodec::new();
        codec.encode(msg.clone(), &mut dst).expect("encode");
        let got = codec.decode(&mut dst).expect("decode ok").expect("a full frame");
        assert_eq!(got, msg);
    }

    let deliver = MsgFrame::Deliver {
        id: "id1".to_string(),
        model: "proj/thing/Changed".to_string(),
        event_nuon: "{a: 1, b: \"x\"}".to_string(),
        files: vec!["out.txt".to_string()],
    };
    let chunk = FileFrame::Chunk {
        id: "id1".to_string(),
        dest: "out.txt".to_string(),
        seq: 3,
        bytes: (0u8..=255).cycle().take(4096).collect(),
        last: true,
    };

    roundtrip_init(InitiatorToAcceptor::Msg(deliver.clone()));
    roundtrip_init(InitiatorToAcceptor::File(chunk.clone()));
    roundtrip_acc(AcceptorToInitiator::Msg(deliver));
    roundtrip_acc(AcceptorToInitiator::File(chunk));
}

/// safe_dest accepts a relative path, rejects an empty one, an absolute one, and
/// any `..` escape from the per-packet inbox.
#[test]
fn safe_dest_rejects_escapes() {
    use crate::safe_dest;
    assert!(safe_dest("out.txt"));
    assert!(safe_dest("sub/dir/out.txt"));
    assert!(!safe_dest(""));
    assert!(!safe_dest("/abs/path"));
    assert!(!safe_dest("../escape"));
    assert!(!safe_dest("sub/../../escape"));
}
