// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_tests;

use sui::address;

#[test]
fun test_bytes_address_roundtrip() {
    let mut ctx = tx_context::dummy();

    let uid0 = object::new(&mut ctx);
    let uid1 = object::new(&mut ctx);

    let addr0 = uid0.to_address();
    let byte0 = uid0.to_bytes();
    let addr1 = uid1.to_address();
    let byte1 = uid1.to_bytes();

    assert!(addr0 != addr1);
    assert!(byte0 != byte1);

    assert!(addr0 == address::from_bytes(byte0));
    assert!(addr1 == address::from_bytes(byte1));

    uid0.delete();
    uid1.delete();
}
