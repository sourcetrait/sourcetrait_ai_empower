#![allow(dead_code)]
//! Entity-pin mTLS verifiers: a peer's leaf must match a configured pin.
use crate::*;

/// Entity-pin verifier: a peer's leaf DER must match the one configured pin.
#[derive(Debug)]
pub(crate) struct EntityPin {
    remote_leaf: rv::CertificateDer<'static>,
    provider: Arc<rv::CryptoProvider>,
}

impl EntityPin {
    pub(crate) fn new(remote_leaf: rv::CertificateDer<'static>) -> Self {
        Self {
            remote_leaf,
            provider: Arc::new(rv::default_provider()),
        }
    }

    fn pinned(&self, end_entity: &rv::CertificateDer<'_>) -> Result<(), rustls::Error> {
        if end_entity.as_ref() == self.remote_leaf.as_ref() {
            Ok(())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rv::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }
}

impl rv::ServerCertVerifier for EntityPin {
    fn verify_server_cert(
        &self,
        end_entity: &rv::CertificateDer<'_>,
        _intermediates: &[rv::CertificateDer<'_>],
        _server_name: &rv::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rv::UnixTime,
    ) -> Result<rv::ServerCertVerified, rustls::Error> {
        self.pinned(end_entity)?;
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

impl rv::ClientCertVerifier for EntityPin {
    fn root_hint_subjects(&self) -> &[rv::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rv::CertificateDer<'_>,
        _intermediates: &[rv::CertificateDer<'_>],
        _now: rv::UnixTime,
    ) -> Result<rv::ClientCertVerified, rustls::Error> {
        self.pinned(end_entity)?;
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

/// The acceptor's union verifier: an inbound leaf must match any configured pin.
#[derive(Debug)]
pub(crate) struct UnionPin {
    remote_leaves: Vec<rv::CertificateDer<'static>>,
    provider: Arc<rv::CryptoProvider>,
}

impl UnionPin {
    pub(crate) fn new(remote_leaves: Vec<rv::CertificateDer<'static>>) -> Self {
        Self {
            remote_leaves,
            provider: Arc::new(rv::default_provider()),
        }
    }

    fn pinned(&self, end_entity: &rv::CertificateDer<'_>) -> Result<(), rustls::Error> {
        if self
            .remote_leaves
            .iter()
            .any(|leaf| leaf.as_ref() == end_entity.as_ref())
        {
            Ok(())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rv::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }
}

impl rv::ClientCertVerifier for UnionPin {
    fn root_hint_subjects(&self) -> &[rv::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rv::CertificateDer<'_>,
        _intermediates: &[rv::CertificateDer<'_>],
        _now: rv::UnixTime,
    ) -> Result<rv::ClientCertVerified, rustls::Error> {
        self.pinned(end_entity)?;
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
