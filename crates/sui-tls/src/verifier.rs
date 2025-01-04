// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::traits::ToFromBytes;
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::pki_types::ServerName;
use rustls::pki_types::SignatureVerificationAlgorithm;
use rustls::pki_types::TrustAnchor;
use rustls::pki_types::UnixTime;
use std::collections::BTreeSet;
use std::sync::Arc;

static SUPPORTED_SIG_ALGS: &[&dyn SignatureVerificationAlgorithm] = &[webpki::ring::ED25519];

static SUPPORTED_ALGORITHMS: WebPkiSupportedAlgorithms = WebPkiSupportedAlgorithms {
    all: SUPPORTED_SIG_ALGS,
    mapping: &[(rustls::SignatureScheme::ED25519, SUPPORTED_SIG_ALGS)],
};

/// The Allower trait provides an interface for callers to inject decsions whether
/// to allow a cert to be verified or not.  This does not prform actual cert validation
/// it only acts as a gatekeeper to decide if we should even try.  For example, we may want
/// to filter our actions to well known public keys.
pub trait Allower: std::fmt::Debug + Send + Sync {
    // TODO: change allower interface to use raw key bytes.
    fn allowed(&self, key: &Ed25519PublicKey) -> bool;
}

/// AllowAll will allow all public certificates to be validated, it fails open
#[derive(Debug, Clone, Default)]
pub struct AllowAll;

impl Allower for AllowAll {
    fn allowed(&self, _: &Ed25519PublicKey) -> bool {
        true
    }
}

/// AllowPublicKeys restricts keys to those that are found in the member set. non-members will
/// not be allowed.
#[derive(Debug, Clone, Default)]
pub struct AllowPublicKeys {
    inner: Arc<ArcSwap<BTreeSet<Ed25519PublicKey>>>,
}

impl AllowPublicKeys {
    pub fn new(allowed: BTreeSet<Ed25519PublicKey>) -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(allowed)),
        }
    }

    pub fn update(&self, new_allowed: BTreeSet<Ed25519PublicKey>) {
        self.inner.store(Arc::new(new_allowed));
    }
}

impl Allower for AllowPublicKeys {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        self.inner.load().contains(key)
    }
}

/// A `rustls::server::ClientCertVerifier` that will ensure that every client provides a valid,
/// expected certificate and that the client's public key is in the validator set.
#[derive(Clone, Debug)]
pub struct ClientCertVerifier<A> {
    allower: A,
    name: String,
}

impl<A> ClientCertVerifier<A> {
    pub fn new(allower: A, name: String) -> Self {
        Self { allower, name }
    }
}

impl<A: Allower + 'static> ClientCertVerifier<A> {
    pub fn rustls_server_config(
        self,
        certificates: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Result<rustls::ServerConfig, rustls::Error> {
        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(std::sync::Arc::new(self))
        .with_single_cert(certificates, private_key)?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        Ok(config)
    }
}

impl<A: Allower> rustls::server::danger::ClientCertVerifier for ClientCertVerifier<A> {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        // Since we're relying on self-signed certificates and not on CAs, continue the handshake
        // without passing a list of CA DNs
        &[]
    }

    // Verifies this is a valid ed25519 self-signed certificate
    // 1. we prepare arguments for webpki's certificate verification (following the rustls implementation)
    //    placing the public key at the root of the certificate chain (as it should be for a self-signed certificate)
    // 2. we call webpki's certificate verification
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer,
        intermediates: &[CertificateDer],
        now: UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        // Step 1: Check this matches the key we expect
        let public_key = public_key_from_certificate(end_entity)?;
        if !self.allower.allowed(&public_key) {
            return Err(rustls::Error::General(format!(
                "invalid certificate: {:?} is not in the validator set",
                public_key,
            )));
        }

        // Step 2: verify the certificate signature and server name with webpki.
        verify_self_signed_cert(
            end_entity,
            intermediates,
            webpki::KeyUsage::client_auth(),
            &self.name,
            now,
        )
        .map(|_| rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &SUPPORTED_ALGORITHMS)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &SUPPORTED_ALGORITHMS)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        SUPPORTED_ALGORITHMS.supported_schemes()
    }
}

/// A `rustls::client::ServerCertVerifier` that ensures the client only connects with the
/// expected server.
#[derive(Clone, Debug)]
pub struct ServerCertVerifier {
    public_key: Ed25519PublicKey,
    name: String,
}

impl ServerCertVerifier {
    pub fn new(public_key: Ed25519PublicKey, name: String) -> Self {
        Self { public_key, name }
    }

    pub fn rustls_client_config_with_client_auth(
        self,
        certificates: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Result<rustls::ClientConfig, rustls::Error> {
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(self))
        .with_client_auth_cert(certificates, private_key)
    }

    pub fn rustls_client_config_with_no_client_auth(
        self,
    ) -> Result<rustls::ClientConfig, rustls::Error> {
        Ok(rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(self))
        .with_no_client_auth())
    }
}

impl rustls::client::danger::ServerCertVerifier for ServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let public_key = public_key_from_certificate(end_entity)?;
        if public_key != self.public_key {
            return Err(rustls::Error::General(format!(
                "invalid certificate: {:?} is not the expected server public key",
                public_key,
            )));
        }

        verify_self_signed_cert(
            end_entity,
            intermediates,
            webpki::KeyUsage::server_auth(),
            &self.name,
            now,
        )
        .map(|_| rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &SUPPORTED_ALGORITHMS)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &SUPPORTED_ALGORITHMS)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        SUPPORTED_ALGORITHMS.supported_schemes()
    }
}

// Verifies this is a valid ed25519 self-signed certificate
// 1. we prepare arguments for webpki's certificate verification (following the rustls implementation)
//    placing the public key at the root of the certificate chain (as it should be for a self-signed certificate)
// 2. we call webpki's certificate verification
fn verify_self_signed_cert(
    end_entity: &CertificateDer,
    intermediates: &[CertificateDer],
    usage: webpki::KeyUsage,
    name: &str,
    now: UnixTime,
) -> Result<(), rustls::Error> {
    // Check we're receiving correctly signed data with the expected key
    // Step 1: prepare arguments
    let (cert, chain, trustroots) = prepare_for_self_signed(end_entity, intermediates)?;

    // Step 2: call verification from webpki
    let verified_cert = cert
        .verify_for_usage(
            SUPPORTED_SIG_ALGS,
            &trustroots,
            chain,
            now,
            usage,
            None,
            None,
        )
        .map_err(pki_error)?;

    // Ensure the cert is valid for the network name
    let subject_name =
        ServerName::try_from(name).map_err(|_| rustls::Error::UnsupportedNameType)?;
    verified_cert
        .end_entity()
        .verify_is_valid_for_subject_name(&subject_name)
        .map_err(pki_error)
}

type CertChainAndRoots<'a> = (
    webpki::EndEntityCert<'a>,
    &'a [CertificateDer<'a>],
    Vec<TrustAnchor<'a>>,
);

// This prepares arguments for webpki, including a trust anchor which is the end entity of the certificate
// (which embodies a self-signed certificate by definition)
fn prepare_for_self_signed<'a>(
    end_entity: &'a CertificateDer,
    intermediates: &'a [CertificateDer],
) -> Result<CertChainAndRoots<'a>, rustls::Error> {
    // EE cert must appear first.
    let cert = webpki::EndEntityCert::try_from(end_entity).map_err(pki_error)?;

    // reinterpret the certificate as a root, materializing the self-signed policy
    let root = webpki::anchor_from_trusted_cert(end_entity).map_err(pki_error)?;

    Ok((cert, intermediates, vec![root]))
}

fn pki_error(error: webpki::Error) -> rustls::Error {
    use webpki::Error::*;
    match error {
        BadDer | BadDerTime => {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        }
        InvalidSignatureForPublicKey
        | UnsupportedSignatureAlgorithm
        | UnsupportedSignatureAlgorithmForPublicKey => {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadSignature)
        }
        CertNotValidForName => {
            rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName)
        }
        e => rustls::Error::General(format!("invalid peer certificate: {e}")),
    }
}

/// Extracts the public key from a certificate.
pub fn public_key_from_certificate(
    certificate: &CertificateDer,
) -> Result<Ed25519PublicKey, rustls::Error> {
    use x509_parser::{certificate::X509Certificate, prelude::FromDer};

    let cert = X509Certificate::from_der(certificate.as_ref())
        .map_err(|e| rustls::Error::General(e.to_string()))?;
    let spki = cert.1.public_key();
    let public_key_bytes =
        <ed25519::pkcs8::PublicKeyBytes as pkcs8::DecodePublicKey>::from_public_key_der(spki.raw)
            .map_err(|e| rustls::Error::General(format!("invalid ed25519 public key: {e}")))?;

    let public_key = Ed25519PublicKey::from_bytes(public_key_bytes.as_ref())
        .map_err(|e| rustls::Error::General(format!("invalid ed25519 public key: {e}")))?;
    Ok(public_key)
}
