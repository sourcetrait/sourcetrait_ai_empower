#![allow(dead_code)]
//! Known-public-key mTLS verifier: a peer's presented leaf DER must match a
//! configured known public key.
use crate::*;

/// Known-public-key verifier: the peer's presented leaf DER must match the one
/// configured known public key, byte for byte. Fills BOTH rustls roles - as the
/// connecting client it verifies the peer's server cert, as the accepting server
/// it verifies the peer's client cert - because each end matches the SAME remote
/// public key regardless of role.
#[derive(Debug)]
pub(crate) struct PublicKeyVerifier {
    remote_public_key: rv::CertificateDer<'static>,
    provider: Arc<rv::CryptoProvider>,
}

impl PublicKeyVerifier {
    pub(crate) fn new(remote_public_key: rv::CertificateDer<'static>) -> Self {
        Self {
            remote_public_key,
            provider: Arc::new(rv::default_provider()),
        }
    }

    fn matches(&self, end_entity: &rv::CertificateDer<'_>) -> Result<(), rustls::Error> {
        if end_entity.as_ref() == self.remote_public_key.as_ref() {
            Ok(())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rv::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }
}

impl rv::ServerCertVerifier for PublicKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rv::CertificateDer<'_>,
        _intermediates: &[rv::CertificateDer<'_>],
        _server_name: &rv::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rv::UnixTime,
    ) -> Result<rv::ServerCertVerified, rustls::Error> {
        self.matches(end_entity)?;
        Ok(rv::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rv::CertificateDer<'_>,
        dss: &rv::DigitallySignedStruct,
    ) -> Result<rv::HandshakeSignatureValid, rustls::Error> {
        rv::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rv::CertificateDer<'_>,
        dss: &rv::DigitallySignedStruct,
    ) -> Result<rv::HandshakeSignatureValid, rustls::Error> {
        rv::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rv::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

impl rv::ClientCertVerifier for PublicKeyVerifier {
    fn root_hint_subjects(&self) -> &[rv::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rv::CertificateDer<'_>,
        _intermediates: &[rv::CertificateDer<'_>],
        _now: rv::UnixTime,
    ) -> Result<rv::ClientCertVerified, rustls::Error> {
        self.matches(end_entity)?;
        Ok(rv::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rv::CertificateDer<'_>,
        dss: &rv::DigitallySignedStruct,
    ) -> Result<rv::HandshakeSignatureValid, rustls::Error> {
        rv::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rv::CertificateDer<'_>,
        dss: &rv::DigitallySignedStruct,
    ) -> Result<rv::HandshakeSignatureValid, rustls::Error> {
        rv::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rv::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}
