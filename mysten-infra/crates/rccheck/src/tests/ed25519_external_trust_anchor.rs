// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use super::*;
use crate::{ed25519_certgen::Ed25519, test_utils::dalek_keypair_strategy, PskSet};

use proptest::prelude::*;

proptest! {
    #[test]
    fn rc_gen_ca_client_split(
        client_kp in dalek_keypair_strategy(),
        ca_kp in dalek_keypair_strategy(),
    ) {
        let subject_alt_names = vec!["localhost".to_string()];

        let ca_public_key = ca_kp.public;

        // 1. Generate CSR on client
        let cert_request =
            Ed25519::keypair_to_der_certificate_request(subject_alt_names, client_kp).unwrap();

        // (2) Client sends CSR to CA

        // CA signs CSR and produces Certificate
        let cert = Ed25519::sign_certificate_request(&cert_request[..], ca_kp).unwrap();

        let ca_spki = Ed25519::public_key_to_spki(&ca_public_key);
        let now = SystemTime::now();

        let psk_set = PskSet::from_der(&[&ca_spki[..]]).unwrap();
        let mut empty = std::iter::empty();

        assert!(psk_set
            .verify_client_cert(&ca_spki, &cert, &[], now)
            .is_ok());
        assert!(psk_set
            .verify_server_cert(
                &ca_spki,
                &cert,
                &[],
                &rustls::ServerName::try_from("localhost").unwrap(),
                &mut empty,
                &[],
                now,
            )
            .is_ok());
    }

    #[test]
    fn rc_gen_ca_client_split_err(
        client_kp in dalek_keypair_strategy(),
        ca_kp in dalek_keypair_strategy()
    ) {
        let client_kp_copy = ed25519_dalek::Keypair::from_bytes(&client_kp.to_bytes()).unwrap();

        let subject_alt_names = vec!["localhost".to_string()];

        let ca_public_key = ca_kp.public;

        // 1. Generate CSR on client
        let cert_request =
            Ed25519::keypair_to_der_certificate_request(subject_alt_names, client_kp).unwrap();

        // Client signs CSR and produces Certificate
        let cert = Ed25519::sign_certificate_request(&cert_request[..], client_kp_copy).unwrap();

        let ca_spki = Ed25519::public_key_to_spki(&ca_public_key);
        let now = SystemTime::now();

        let psk_set = PskSet::from_der(&[&ca_spki[..]]).unwrap();
        let mut empty = std::iter::empty();

        assert!(psk_set
            .verify_client_cert(&ca_spki, &cert, &[], now)
            .is_err());
        assert!(psk_set
            .verify_server_cert(
                &ca_spki,
                &cert,
                &[],
                &rustls::ServerName::try_from("localhost").unwrap(),
                &mut empty,
                &[],
                now,
            )
            .is_err());
    }
}
