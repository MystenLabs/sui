// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use pkcs8::EncodePrivateKey;
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;

pub struct SelfSignedCertificate {
    inner: rcgen::Certificate,
    key: KeyPair,
}

impl SelfSignedCertificate {
    pub fn new(private_key: Ed25519PrivateKey, server_name: &str) -> Self {
        let (cert, key) = generate_self_signed_tls_certificate(private_key, server_name);
        Self { inner: cert, key }
    }

    pub fn rustls_certificate(&self) -> CertificateDer<'static> {
        self.inner.der().to_owned()
    }

    pub fn rustls_private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key.serialize_der().into())
    }

    pub fn reqwest_identity(&self) -> reqwest::tls::Identity {
        let pem = self.inner.pem() + &self.key.serialize_pem();
        reqwest::tls::Identity::from_pem(pem.as_ref()).unwrap()
    }

    pub fn reqwest_certificate(&self) -> reqwest::tls::Certificate {
        reqwest::tls::Certificate::from_der(self.inner.der()).unwrap()
    }
}

fn generate_self_signed_tls_certificate(
    private_key: Ed25519PrivateKey,
    server_name: &str,
) -> (rcgen::Certificate, KeyPair) {
    let keypair = ed25519::KeypairBytes {
        secret_key: private_key.0.to_bytes(),
        // ring cannot handle the optional public key that would be legal der here
        // that is, ring expects PKCS#8 v.1
        public_key: None,
    };

    let pkcs8 = keypair.to_pkcs8_der().unwrap();
    let key_der = PrivateKeyDer::Pkcs8(pkcs8.as_bytes().to_vec().into());
    let keypair = KeyPair::from_der_and_sign_algo(&key_der, &rcgen::PKCS_ED25519).unwrap();

    (generate_cert(&keypair, server_name), keypair)
}

fn generate_cert(keypair: &KeyPair, server_name: &str) -> rcgen::Certificate {
    CertificateParams::new(vec![server_name.to_owned()])
        .unwrap()
        .self_signed(keypair)
        .expect(
            "unreachable! from_params should only fail if the key is incompatible with params.algo",
        )
}

pub(crate) fn public_key_from_certificate(
    certificate: &CertificateDer,
) -> Result<Ed25519PublicKey, anyhow::Error> {
    use fastcrypto::traits::ToFromBytes;
    use x509_parser::{certificate::X509Certificate, prelude::FromDer};

    let cert = X509Certificate::from_der(certificate.as_ref())
        .map_err(|e| rustls::Error::General(e.to_string()))?;
    let spki = cert.1.public_key();
    let public_key_bytes =
        <ed25519::pkcs8::PublicKeyBytes as pkcs8::DecodePublicKey>::from_public_key_der(spki.raw)
            .map_err(|e| rustls::Error::General(format!("invalid ed25519 public key: {e}")))?;

    let public_key = Ed25519PublicKey::from_bytes(public_key_bytes.as_ref())?;

    Ok(public_key)
}
