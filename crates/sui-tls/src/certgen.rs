// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519PrivateKey;
use pkcs8::EncodePrivateKey;
use rcgen::{CertificateParams, KeyPair, SignatureAlgorithm};

pub fn generate_self_signed_tls_certificate(
    private_key: Ed25519PrivateKey,
) -> (rustls::Certificate, rustls::PrivateKey) {
    let keypair = ed25519::KeypairBytes {
        secret_key: private_key.0.to_bytes(),
        // ring cannot handle the optional public key that would be legal der here
        // that is, ring expects PKCS#8 v.1
        public_key: None,
    };

    generate_cert(&keypair, crate::TLS_CERTIFICATE_SERVER_NAME)
}

fn generate_cert(
    keypair: &ed25519::KeypairBytes,
    server_name: &str,
) -> (rustls::Certificate, rustls::PrivateKey) {
    let pkcs8 = keypair.to_pkcs8_der().unwrap();
    let key_der = rustls::PrivateKey(pkcs8.as_bytes().to_vec());
    let certificate = private_key_to_certificate(vec![server_name.to_owned()], &key_der).unwrap();
    (certificate, key_der)
}

fn private_key_to_certificate(
    subject_names: impl Into<Vec<String>>,
    private_key: &rustls::PrivateKey,
) -> Result<rustls::Certificate, anyhow::Error> {
    let alg = &rcgen::PKCS_ED25519;

    let certificate = gen_certificate(subject_names, (private_key.0.as_ref(), alg))?;
    Ok(certificate)
}

fn gen_certificate(
    subject_names: impl Into<Vec<String>>,
    key_pair: (&[u8], &'static SignatureAlgorithm),
) -> Result<rustls::Certificate, anyhow::Error> {
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
