//! Unit tests for the remote-link entity-pin verifiers.

use crate::rv;
use crate::server::remote::verify::EntityPin;
use rustls::client::danger::ServerCertVerifier;
use rustls::server::danger::ClientCertVerifier;

/// A leaf byte-identical to the pin is accepted; any other leaf is rejected -
/// both as a server cert (client-side) and a client cert (server-side).
#[test]
fn entity_pin_accepts_only_the_pinned_leaf() {
    let pin_bytes = vec![1u8, 2, 3, 4, 5];
    let other_bytes = vec![9u8, 9, 9];
    let pin = rv::CertificateDer::from(pin_bytes.clone());
    let verifier = EntityPin::new(pin);

    let same = rv::CertificateDer::from(pin_bytes);
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
