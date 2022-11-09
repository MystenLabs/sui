// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use crate::{
    test_utils::{dalek_keypair_strategy, dalek_pubkey_strategy},
    Certifiable, Psk,
};

use super::*;

use proptest::prelude::*;
use rustls::{client::ServerCertVerifier, server::ClientCertVerifier};

proptest! {
    #[test]
    fn ed25519_keys_to_spki(
        pub_key in dalek_pubkey_strategy(),
    ){
        let spki = Ed25519::public_key_to_spki(&pub_key);
        let psk = Psk::from_der(&spki);
        psk.unwrap();
    }

    #[test]
    fn rc_gen_self_signed_dalek(
        kp in dalek_keypair_strategy(),
    ) {
        let subject_alt_names = vec!["localhost".to_string()];
        let public_key = kp.public;

        let cert = Ed25519::keypair_to_certificate(subject_alt_names, kp).unwrap();

        let spki = Ed25519::public_key_to_spki(&public_key);
        let psk = Psk::from_der(&spki).unwrap();
        let now = SystemTime::now();

        // this passes client verification
        psk.verify_client_cert(&cert, &[], now).unwrap();

        // this passes server verification
        let mut empty = std::iter::empty();
        psk.verify_server_cert(
            &cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now,
        )
        .unwrap();
    }

    #[test]
    fn rc_gen_not_self_signed_dalek(
        // this time the pubkey does not match the creation keypair
        kp in dalek_keypair_strategy(),
        public_key in dalek_pubkey_strategy(),
    ) {
        let subject_alt_names = vec!["localhost".to_string()];

        let cert = Ed25519::keypair_to_certificate(subject_alt_names, kp).unwrap();

        let spki = Ed25519::public_key_to_spki(&public_key);
        let psk = Psk::from_der(&spki).unwrap();
        let now = SystemTime::now();

        // this does not pass client verification
        assert!(psk.verify_client_cert(&cert, &[], now).err()
            .unwrap()
            .to_string()
            .contains("invalid peer certificate"));

        // this does not pass server verification
        let mut empty = std::iter::empty();
        assert!(psk.verify_server_cert(
            &cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now,
        )
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
    }
}
