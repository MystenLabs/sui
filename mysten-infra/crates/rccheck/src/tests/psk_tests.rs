// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{test_utils::cert_bytes_to_spki_bytes, *};
use rcgen::generate_simple_self_signed;
use rustls::{client::ServerCertVerifier, server::ClientCertVerifier};

#[test]
fn serde_round_trip() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let spki = cert_bytes_to_spki_bytes(&cert_bytes);
    let psk = Psk::from_der(&spki).unwrap();
    let psk_bytes = bincode::serialize(&psk).unwrap();
    let psk_roundtripped = bincode::deserialize::<Psk>(&psk_bytes).unwrap();
    assert_eq!(psk, psk_roundtripped);
}

#[test]
fn rc_gen_self_client() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();
    let spki = cert_bytes_to_spki_bytes(&cert_bytes);
    let psk = Psk::from_der(&spki).unwrap();

    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    assert!(psk.verify_client_cert(&rstls_cert, &[], now).is_ok());
}

#[test]
fn rc_gen_not_self_client() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names.clone()).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let other_cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let other_bytes: Vec<u8> = other_cert.serialize_der().unwrap();
    let spki = cert_bytes_to_spki_bytes(&other_bytes);
    let psk = Psk::from_der(&spki).unwrap();

    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    assert!(psk
        .verify_client_cert(&rstls_cert, &[], now)
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
}

#[test]
fn rc_gen_self_server() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();
    let spki = cert_bytes_to_spki_bytes(&cert_bytes);
    let psk = Psk::from_der(&spki).unwrap();

    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    let mut empty = std::iter::empty();

    assert!(psk
        .verify_server_cert(
            &rstls_cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now
        )
        .is_ok());
}

#[test]
fn rc_gen_not_self_server() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names.clone()).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let other_cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let other_bytes: Vec<u8> = other_cert.serialize_der().unwrap();
    let spki = cert_bytes_to_spki_bytes(&other_bytes);
    let psk = Psk::from_der(&spki).unwrap();
    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    let mut empty = std::iter::empty();

    assert!(psk
        .verify_server_cert(
            &rstls_cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now
        )
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
}
