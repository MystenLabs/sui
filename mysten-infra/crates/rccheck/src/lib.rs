// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The purpose of this module is to afford the user that already has public key infrastructure on their hosts [^1] the ability to perform
//! authentication with TLS certificates (more specifically using rustls) using those pre-distributed public keys alone. The Public keys are hence deemed pre-shared.
//!
//! [^1]: i.e. each host knows the public keys of all the other hosts
//!
//! This module hence helps with the creation of rustls Client and Server verifiers which expect the certificate they verify to be a self-signed certificate,
//! signed by an expected public key in the form of an X509 SubjectPublicKeyInfo element.
//!
//! In certgen, We also offer a trait `Certifiable` (and convenience implementation) that closes the loop: it can convert a key pair into a valid self-signed certificate,
//! and a public key of the same format into some X509 SubjectPublicKeyInfo.

use std::{fmt, time::SystemTime};

use ouroboros::self_referencing;
use rustls::{
    client::{ServerCertVerified, ServerCertVerifier},
    server::{ClientCertVerified, ClientCertVerifier},
};
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize,
};
use x509_parser::certificate::X509Certificate;
use x509_parser::{traits::FromDer, x509::SubjectPublicKeyInfo};

#[cfg(test)]
#[path = "tests/psk_tests.rs"]
mod psk_tests;

#[cfg(test)]
#[path = "tests/test_utils.rs"]
pub(crate) mod test_utils;

pub mod ed25519_certgen;

// Re-export our version of rustls to stave off compatiblity issues
pub use rustls;

type SignatureAlgorithms = &'static [&'static webpki::SignatureAlgorithm];
static SUPPORTED_SIG_ALGS: SignatureAlgorithms = &[&webpki::ECDSA_P256_SHA256, &webpki::ED25519];

/// A trait that offers the key conversions necessary for generating and verifying self-signed certificates matching an expected key
pub trait Certifiable {
    type PublicKey;
    type KeyPair;

    /// This generates a self-signed X509 certificate with the provided key pair
    fn keypair_to_certificate(
        subject_names: impl Into<Vec<String>>,
        kp: Self::KeyPair,
    ) -> Result<rustls::Certificate, anyhow::Error>;

    /// This generates X.509 `SubjectPublicKeyInfo` (SPKI) bytes (in DER format) from the provided public key
    fn public_key_to_spki(public_key: &Self::PublicKey) -> Vec<u8>;
}

/// X.509 `SubjectPublicKeyInfo` (SPKI) as defined in [RFC 5280 Section 4.1.2.7].
///
/// ASN.1 structure containing an [`AlgorithmIdentifier`] and public key
/// data in an algorithm specific format.
///
/// ```text
///    SubjectPublicKeyInfo  ::=  SEQUENCE  {
///         algorithm            AlgorithmIdentifier,
///         subjectPublicKey     BIT STRING  }
/// ```
///
/// [RFC 5280 Section 4.1.2.7]: https://tools.ietf.org/html/rfc5280#section-4.1.2.7
///
/// We only support ECDSA P-256 & Ed25519 (for now).
///
/// Example
/// ```
/// use rccheck::*;
/// let mut rng = rand::thread_rng();
/// let keypair = ed25519_dalek::Keypair::generate(&mut rng);
/// let spki = ed25519_certgen::Ed25519::public_key_to_spki(&keypair.public);
/// let psk = Psk::from_der(&spki).unwrap();
/// ```
///
#[self_referencing]
#[derive(PartialEq, Debug)]
pub struct Psk {
    pub key_bytes: Vec<u8>,
    #[covariant]
    #[borrows(key_bytes)]
    pub spki: SubjectPublicKeyInfo<'this>,
}

impl Eq for Psk {}

impl Psk {
    pub fn from_der(bytes: &[u8]) -> Result<Psk, anyhow::Error> {
        PskTryBuilder {
            key_bytes: bytes.to_vec(),
            spki_builder: |key_bytes: &Vec<u8>| {
                SubjectPublicKeyInfo::from_der(key_bytes)
                    .map(|(_, spki)| spki)
                    .map_err(|e| e.into())
            },
        }
        .try_build()
    }
}

impl Clone for Psk {
    fn clone(&self) -> Self {
        // unwrap safe as the bytes match
        Self::from_der(self.borrow_key_bytes()).unwrap()
    }
}

////////////////////////////////////////////////////////////////
/// Ser/de
////////////////////////////////////////////////////////////////

impl Serialize for Psk {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.borrow_key_bytes())
    }
}

struct DerBytesVisitor;

impl<'de> Visitor<'de> for DerBytesVisitor {
    type Value = Psk;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a borrowed Subject Public Key Info in DER format")
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Psk::from_der(v).map_err(Error::custom)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Psk::from_der(v.as_bytes()).map_err(Error::custom)
    }
}

impl<'de> Deserialize<'de> for Psk {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_bytes(DerBytesVisitor)
    }
}

////////////////////////////////////////////////////////////////
/// end Ser/de
////////////////////////////////////////////////////////////////

/// A `ClientCertVerifier` that will ensure that every client provides a valid, expected
/// certificate, without any name checking.
impl ClientCertVerifier for Psk {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> Option<bool> {
        Some(true)
    }

    fn client_auth_root_subjects(&self) -> Option<rustls::DistinguishedNames> {
        // We can't guarantee subjects before having seen the cert. This should not be None for compatiblity reasons
        Some(rustls::DistinguishedNames::new())
    }

    // Verifies this is a valid certificate self-signed by the public key we expect(in PSK)
    // 1. we check the equality of the certificate's public key with the key we expect
    // 2. we prepare arguments for webpki's certificate verification (following the rustls implementation)
    //    placing the public key at the root of the certificate chain (as it should be for a self-signed certificate)
    // 3. we call webpki's certificate verification
    fn verify_client_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        now: SystemTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        // Step 1: Check this matches the key we expect
        let cert = X509Certificate::from_der(&end_entity.0[..])
            .map_err(|_| rustls::Error::InvalidCertificateEncoding)?;
        let spki = cert.1.public_key().clone();
        if &spki != self.borrow_spki() {
            return Err(rustls::Error::InvalidCertificateData(format!(
                "invalid peer certificate: received {:?} instead of expected {:?}",
                spki,
                self.borrow_spki()
            )));
        }

        // We now check we're receiving correctly signed data with the expected key
        // Step 2: prepare arguments
        let (cert, chain, trustroots) = prepare_for_self_signed(end_entity, intermediates)?;
        let now = webpki::Time::try_from(now).map_err(|_| rustls::Error::FailedToGetCurrentTime)?;

        // Step 3: call verification from webpki
        cert.verify_is_valid_tls_client_cert(
            SUPPORTED_SIG_ALGS,
            &webpki::TlsClientTrustAnchors(&trustroots),
            &chain,
            now,
        )
        .map_err(pki_error)
        .map(|_| ClientCertVerified::assertion())
    }
}

impl<'a> ServerCertVerifier for Psk {
    // Verifies this is a valid certificate self-signed by the public key we expect(in PSK)
    // 1. we check the equality of the certificate's public key with the key we expect
    // 2. we prepare arguments for webpki's certificate verification (following the rustls implementation)
    //    placing the public key at the root of the certificate chain (as it should be for a self-signed certificate)
    // 3. we call webpki's certificate verification
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        server_name: &rustls::ServerName,
        scts: &mut dyn Iterator<Item = &[u8]>,
        ocsp_response: &[u8],
        now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // Step 1: Check this matches the key we expect
        let cert = X509Certificate::from_der(&end_entity.0[..])
            .map_err(|_| rustls::Error::InvalidCertificateEncoding)?;
        let spki = cert.1.public_key().clone();
        if &spki != self.borrow_spki() {
            return Err(rustls::Error::InvalidCertificateData(format!(
                "invalid peer certificate: received {:?} instead of expected {:?}",
                spki,
                self.borrow_spki()
            )));
        }

        // Then we check this is actually a valid self-signed certificate with matching name
        // Step 2: prepare arguments
        let (cert, chain, trustroots) = prepare_for_self_signed(end_entity, intermediates)?;
        let webpki_now =
            webpki::Time::try_from(now).map_err(|_| rustls::Error::FailedToGetCurrentTime)?;

        let dns_nameref = match server_name {
            rustls::ServerName::DnsName(dns_name) => {
                webpki::DnsNameRef::try_from_ascii_str(dns_name.as_ref())
                    .map_err(|_| rustls::Error::UnsupportedNameType)?
            }
            _ => return Err(rustls::Error::UnsupportedNameType),
        };

        // Step 3: call verification from webpki
        let cert = cert
            .verify_is_valid_tls_server_cert(
                SUPPORTED_SIG_ALGS,
                &webpki::TlsServerTrustAnchors(&trustroots),
                &chain,
                webpki_now,
            )
            .map_err(pki_error)
            .map(|_| cert)?;

        // log additional certificate transaparency info (which is pointless in our self-signed context) and return
        let mut peekable = scts.peekable();
        if peekable.peek().is_none() {
            tracing::trace!("Met unvalidated certificate transparency data");
        }

        if !ocsp_response.is_empty() {
            tracing::trace!("Unvalidated OCSP response: {:?}", ocsp_response.to_vec());
        }

        cert.verify_is_valid_for_dns_name(dns_nameref)
            .map_err(pki_error)
            .map(|_| ServerCertVerified::assertion())
    }
}

type CertChainAndRoots<'a> = (
    webpki::EndEntityCert<'a>,
    Vec<&'a [u8]>,
    Vec<webpki::TrustAnchor<'a>>,
);

// This prepares arguments for webpki, including a trust anchor which is the end entity of the certificate
// (which embodies a self-signed certificate by definition)
fn prepare_for_self_signed<'a>(
    end_entity: &'a rustls::Certificate,
    intermediates: &'a [rustls::Certificate],
) -> Result<CertChainAndRoots<'a>, rustls::Error> {
    // EE cert must appear first.
    let cert = webpki::EndEntityCert::try_from(end_entity.0.as_ref()).map_err(pki_error)?;

    let intermediates: Vec<&'a [u8]> = intermediates.iter().map(|cert| cert.0.as_ref()).collect();

    // reinterpret the certificate as a root, materializing the self-signed policy
    let root = webpki::TrustAnchor::try_from_cert_der(end_entity.0.as_ref()).map_err(pki_error)?;

    Ok((cert, intermediates, vec![root]))
}

fn pki_error(error: webpki::Error) -> rustls::Error {
    use webpki::Error::*;
    match error {
        BadDer | BadDerTime => rustls::Error::InvalidCertificateEncoding,
        InvalidSignatureForPublicKey => rustls::Error::InvalidCertificateSignature,
        UnsupportedSignatureAlgorithm | UnsupportedSignatureAlgorithmForPublicKey => {
            rustls::Error::InvalidCertificateSignatureType
        }
        e => rustls::Error::InvalidCertificateData(format!("invalid peer certificate: {e}")),
    }
}
