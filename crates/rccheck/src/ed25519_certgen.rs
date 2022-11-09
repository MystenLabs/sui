// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ed25519::pkcs8::EncodePrivateKey;

use pkcs8::der::Encode;
use rcgen::{CertificateParams, KeyPair, SignatureAlgorithm};

use crate::Certifiable;

#[cfg(test)]
#[path = "tests/ed25519_certgen_tests.rs"]
mod ed25519_certgen_tests;

#[cfg(test)]
#[path = "tests/ed25519_external_trust_anchor.rs"]
mod ed25519_external_trust_anchor;

fn dalek_to_keypair_bytes(dalek_kp: ed25519_dalek::Keypair) -> ed25519::KeypairBytes {
    let private = dalek_kp.secret;
    let _public = dalek_kp.public;

    ed25519::KeypairBytes {
        secret_key: private.to_bytes(),
        // ring cannot handle the optional public key that would be legal der here
        // that is, ring expects PKCS#8 v.1
        public_key: None, // Some(_public.to_bytes()),
    }
}

fn keypair_bytes_to_pkcs8_n_algo(
    kpb: ed25519::KeypairBytes,
) -> Result<(pkcs8::der::SecretDocument, &'static SignatureAlgorithm), pkcs8::Error> {
    // PKCS#8 v2 as described in [RFC 5958].
    // PKCS#8 v2 keys include an additional public key field.
    let pkcs8 = kpb.to_pkcs8_der()?;

    Ok((pkcs8, &rcgen::PKCS_ED25519))
}

fn gen_certificate(
    subject_names: impl Into<Vec<String>>,
    key_pair: (&[u8], &'static SignatureAlgorithm),
) -> Result<rustls::Certificate, eyre::Report> {
    let kp = KeyPair::from_der_and_sign_algo(key_pair.0, key_pair.1)?;

    let mut cert_params = CertificateParams::new(subject_names);
    cert_params.key_pair = Some(kp);
    cert_params.distinguished_name = rcgen::DistinguishedName::new();
    cert_params.alg = key_pair.1;

    let cert = rcgen::Certificate::from_params(cert_params).expect(
        "unreachable! from_params should only fail if the key is incompatible with params.algo",
    );
    let cert_bytes = cert.serialize_der()?;
    Ok(rustls::Certificate(cert_bytes))
}

/// Signs a CertificateSigningRequest (CSR) and produces a X.509 Certificate.
fn sign_certificate_signing_request(
    csr_der: &[u8], // DER-serialized CSR
    key_pair: (&[u8], &'static SignatureAlgorithm),
) -> Result<rustls::Certificate, eyre::Report> {
    let kp = KeyPair::from_der_and_sign_algo(key_pair.0, key_pair.1)?;

    let csr = rcgen::CertificateSigningRequest::from_der(csr_der).map_err(eyre::Report::new)?;

    let mut cert_params = CertificateParams::new(vec!["localhost".to_string()]);
    cert_params.key_pair = Some(kp);
    cert_params.distinguished_name = rcgen::DistinguishedName::new();
    cert_params.alg = key_pair.1;

    let cert = rcgen::Certificate::from_params(cert_params).expect(
        "unreachable! from_params should only fail if the key is incompatible with params.algo",
    );

    let signed_cert = csr
        .serialize_der_with_signer(&cert)
        .map_err(eyre::Report::new)?;
    Ok(rustls::Certificate(signed_cert))
}

// Token struct to peg this purely functional impl on
pub struct Ed25519 {}
impl Certifiable for Ed25519 {
    type PublicKey = ed25519_dalek::PublicKey;

    type KeyPair = ed25519_dalek::Keypair;

    /// KISS function to generate a self signed certificate from a dalek keypair
    /// Given a set of domain names you want your certificate to be valid for, this function fills in the other generation parameters with
    /// reasonable defaults and generates a self signed certificate using the keypair passed as argument as output.
    ///
    /// ## Example
    /// ```
    /// extern crate rccheck;
    /// use rccheck::ed25519_certgen::Ed25519;
    /// use rccheck::Certifiable;
    /// # let mut rng = rand::thread_rng();
    /// let subject_alt_names = vec!["localhost".to_string()];
    /// let kp = ed25519_dalek::Keypair::generate(&mut rng);
    ///
    /// let cert = Ed25519::keypair_to_certificate(subject_alt_names, kp).unwrap();
    /// // The certificate is now valid for localhost
    /// ```
    ///
    fn keypair_to_certificate(
        subject_names: impl Into<Vec<String>>,
        kp: Self::KeyPair,
    ) -> Result<rustls::Certificate, eyre::Report> {
        let keypair_bytes = dalek_to_keypair_bytes(kp);
        let (pkcs_bytes, alg) =
            keypair_bytes_to_pkcs8_n_algo(keypair_bytes).map_err(eyre::Report::new)?;

        let certificate = gen_certificate(subject_names, (pkcs_bytes.as_bytes(), alg))?;
        Ok(certificate)
    }

    /// This produces X.509 `SubjectPublicKeyInfo` (SPKI) as defined in [RFC 5280 Section 4.1.2.7](https://datatracker.ietf.org/doc/html/rfc5280).
    /// in DER-encoded format, serialized to a byte string.
    /// Example
    /// ```
    /// use rccheck::*;
    /// let mut rng = rand::thread_rng();
    /// let keypair = ed25519_dalek::Keypair::generate(&mut rng);
    /// let spki = ed25519_certgen::Ed25519::public_key_to_spki(&keypair.public); // readable by Psk::from_der
    /// ```
    fn public_key_to_spki(public_key: &Self::PublicKey) -> Vec<u8> {
        let subject_public_key = public_key.as_bytes();

        let key_info = pkcs8::spki::SubjectPublicKeyInfo {
            algorithm: pkcs8::spki::AlgorithmIdentifier {
                // ed25519 OID
                oid: ed25519::pkcs8::ALGORITHM_OID,
                // some environments require a type ASN.1 NULL, use the commented alternative if so
                // this instead matches our rcgen-produced certificates for compatibiltiy
                // use pkcs8::spki::der::asn1;
                parameters: None, // Some(asn1::Any::from(asn1::Null)),
            },
            subject_public_key,
        };

        // Infallible because we know the public key is valid.
        key_info
            .to_vec()
            .expect("Dalek public key should be valid!")
    }

    fn sign_certificate_request(
        certificate_request_der: &[u8],
        kp: Self::KeyPair,
    ) -> Result<rustls::Certificate, eyre::Report> {
        let keypair_bytes = dalek_to_keypair_bytes(kp);
        let (pkcs_bytes, alg) =
            keypair_bytes_to_pkcs8_n_algo(keypair_bytes).map_err(eyre::Report::new)?;

        sign_certificate_signing_request(certificate_request_der, (pkcs_bytes.as_bytes(), alg))
    }

    fn keypair_to_der_certificate_request(
        subject_names: impl Into<Vec<String>>,
        kp: Self::KeyPair,
    ) -> Result<Vec<u8>, eyre::Report> {
        let keypair_bytes = dalek_to_keypair_bytes(kp);
        let (pkcs_bytes, alg) =
            keypair_bytes_to_pkcs8_n_algo(keypair_bytes).map_err(eyre::Report::new)?;
        let kp = KeyPair::from_der_and_sign_algo(pkcs_bytes.as_bytes(), alg)?;

        let mut cert_params = CertificateParams::new(subject_names.into());
        cert_params.key_pair = Some(kp);
        cert_params.distinguished_name = rcgen::DistinguishedName::new();
        cert_params.alg = alg;

        let cert = rcgen::Certificate::from_params(cert_params).expect(
            "unreachable! from_params should only fail if the key is incompatible with params.algo",
        );
        Ok(cert.serialize_request_der()?)
    }
}
