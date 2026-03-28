// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::object_salted_id_tests;

#[test]
fun test_deterministic() {
    let mut ctx = sui::tx_context::dummy();

    let uid1 = sui::object::new_with_salt(b"my_salt", &mut ctx);
    let uid2 = sui::object::new_with_salt(b"my_salt", &mut ctx);

    assert!(uid1.to_address() == uid2.to_address());

    sui::object::delete(uid1);
    sui::object::delete(uid2);
}

#[test]
fun test_different_salt() {
    let mut ctx = sui::tx_context::dummy();

    let uid1 = sui::object::new_with_salt(b"salt_a", &mut ctx);
    let uid2 = sui::object::new_with_salt(b"salt_b", &mut ctx);

    assert!(uid1.to_address() != uid2.to_address());

    sui::object::delete(uid1);
    sui::object::delete(uid2);
}

#[test]
fun test_different_creator() {
    // Verify that different creator addresses yield different IDs for the same salt.
    // We exercise new_with_salt_as with two distinct proof UIDs as stand-ins for
    // two different factory objects.
    let mut ctx = sui::tx_context::dummy();

    let proof_a = sui::object::new(&mut ctx);
    let proof_b = sui::object::new(&mut ctx);
    let creator_a = sui::object::uid_to_address(&proof_a);
    let creator_b = sui::object::uid_to_address(&proof_b);

    let uid_a = sui::object::new_with_salt_as(creator_a, &proof_a, b"salt", &mut ctx);
    let uid_b = sui::object::new_with_salt_as(creator_b, &proof_b, b"salt", &mut ctx);

    assert!(uid_a.to_address() != uid_b.to_address());

    sui::object::delete(uid_a);
    sui::object::delete(uid_b);
    sui::object::delete(proof_a);
    sui::object::delete(proof_b);
}
