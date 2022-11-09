// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::prelude::*;
use proptest::strategy::Strategy;
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

///
/// Proptest Helpers
///

pub fn dalek_keypair_strategy() -> impl Strategy<Value = ed25519_dalek::Keypair> {
    any::<[u8; 32]>()
        .prop_map(|seed| {
            let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_seed(seed);
            ed25519_dalek::Keypair::generate(&mut rng)
        })
        .no_shrink()
}

pub fn dalek_pubkey_strategy() -> impl Strategy<Value = ed25519_dalek::PublicKey> {
    dalek_keypair_strategy().prop_map(|v| v.public)
}

///
/// Misc Helpers
///

pub fn cert_bytes_to_spki_bytes(cert_bytes: &[u8]) -> Vec<u8> {
    let cert_parsed = X509Certificate::from_der(cert_bytes)
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = cert_parsed.1.public_key().clone();
    spki.raw.to_vec()
}
