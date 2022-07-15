// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ed25519::Ed25519PublicKeyBytes, traits::ToFromBytes};

#[test]
fn test_public_key_bytes_to_from() {
    let pk = [
        0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3, 4, 5,
        6, 7,
    ];
    let pubkey_bytes = Ed25519PublicKeyBytes::new(pk);
    let pubkey_clone = Ed25519PublicKeyBytes::from_bytes(pubkey_bytes.as_bytes()).unwrap();
    assert_eq!(pubkey_bytes, pubkey_clone);
}
