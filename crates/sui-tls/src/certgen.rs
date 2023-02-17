// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use pkcs8::EncodePrivateKey;
use rcgen::{CertificateParams, KeyPair, SignatureAlgorithm};

pub struct SelfSignedCertificate {
    inner: rcgen::Certificate,
}

impl SelfSignedCertificate {
    pub fn new(private_key: Ed25519PrivateKey, server_name: &str) -> Self {
        Self {
            inner: generate_self_signed_tls_certificate(private_key, server_name),
        }
    }

    pub fn rustls_certificate(&self) -> rustls::Certificate {
        let cert_bytes = self.inner.serialize_der().unwrap();
        rustls::Certificate(cert_bytes)
    }

    pub fn rustls_private_key(&self) -> rustls::PrivateKey {
        let private_key_bytes = self.inner.serialize_private_key_der();
        rustls::PrivateKey(private_key_bytes)
    }

    pub fn reqwest_identity(&self) -> reqwest::tls::Identity {
        let pem = self.inner.serialize_pem().unwrap() + &self.inner.serialize_private_key_pem();
        reqwest::tls::Identity::from_pem(pem.as_ref()).unwrap()
    }

    pub fn reqwest_certificate(&self) -> reqwest::tls::Certificate {
        let cert = self.inner.serialize_der().unwrap();
        reqwest::tls::Certificate::from_der(&cert).unwrap()
    }
}

fn generate_self_signed_tls_certificate(
    private_key: Ed25519PrivateKey,
    server_name: &str,
) -> rcgen::Certificate {
    let keypair = ed25519::KeypairBytes {
        secret_key: private_key.0.to_bytes(),
        // ring cannot handle the optional public key that would be legal der here
        // that is, ring expects PKCS#8 v.1
        public_key: None,
    };

    generate_cert(&keypair, server_name)
}

fn generate_cert(keypair: &ed25519::KeypairBytes, server_name: &str) -> rcgen::Certificate {
    let pkcs8 = keypair.to_pkcs8_der().unwrap();
    let key_der = rustls::PrivateKey(pkcs8.as_bytes().to_vec());
    private_key_to_certificate(vec![server_name.to_owned()], &key_der).unwrap()
}

fn private_key_to_certificate(
    subject_names: impl Into<Vec<String>>,
    private_key: &rustls::PrivateKey,
) -> Result<rcgen::Certificate, anyhow::Error> {
    let alg = &rcgen::PKCS_ED25519;

    let certificate = gen_certificate(subject_names, (private_key.0.as_ref(), alg))?;
    Ok(certificate)
}

fn gen_certificate(
    subject_names: impl Into<Vec<String>>,
    key_pair: (&[u8], &'static SignatureAlgorithm),
) -> Result<rcgen::Certificate, anyhow::Error> {
    let kp = KeyPair::from_der_and_sign_algo(key_pair.0, key_pair.1)?;

    let mut cert_params = CertificateParams::new(subject_names);
    cert_params.key_pair = Some(kp);
    cert_params.distinguished_name = rcgen::DistinguishedName::new();
    cert_params.alg = key_pair.1;

    let cert = rcgen::Certificate::from_params(cert_params).expect(
        "unreachable! from_params should only fail if the key is incompatible with params.algo",
    );
    Ok(cert)
}

pub(crate) fn public_key_from_certificate(
    certificate: &rustls::Certificate,
) -> Result<Ed25519PublicKey, anyhow::Error> {
    use fastcrypto::traits::ToFromBytes;
    use x509_parser::{certificate::X509Certificate, prelude::FromDer};

    let cert = X509Certificate::from_der(certificate.0.as_ref())
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)?;
    let spki = cert.1.public_key();
    let public_key_bytes =
        <ed25519::pkcs8::PublicKeyBytes as pkcs8::DecodePublicKey>::from_public_key_der(spki.raw)
            .map_err(|e| {
            rustls::Error::InvalidCertificateData(format!("invalid ed25519 public key: {e}"))
        })?;

    let public_key = Ed25519PublicKey::from_bytes(public_key_bytes.as_ref())?;

    Ok(public_key)
}
