// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::{ed25519::Ed25519PublicKeyBytes, traits::ToFromBytes};

#[test]
fn test_public_key_bytes_to_from() {
    let pk = [
        0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5,
        6, 7,
    ];
    let pubkey_bytes = Ed25519PublicKeyBytes::new(pk);
    let pubkey_clone = Ed25519PublicKeyBytes::from_bytes(pubkey_bytes.as_bytes()).unwrap();
    let pubkey_clone_2 = Ed25519PublicKeyBytes::from_bytes(pubkey_bytes.as_ref()).unwrap();

    assert_eq!(pubkey_bytes, pubkey_clone);
    assert_eq!(pubkey_bytes, pubkey_clone_2);
}

#[test]
fn test_from_str() {
    const HEX_STR: &str = "73eda753299d7d483339d80809a1d80553bda402fffe5bfefffffffe00000000";
    let bytes: [u8; 32] =
        hex_literal::hex!("73eda753299d7d483339d80809a1d80553bda402fffe5bfefffffffe00000000");

    let pubkey_bytes = Ed25519PublicKeyBytes::from_str(format!("0x{}", HEX_STR).as_str()).unwrap();
    assert_eq!(pubkey_bytes.as_ref(), &bytes);
}
