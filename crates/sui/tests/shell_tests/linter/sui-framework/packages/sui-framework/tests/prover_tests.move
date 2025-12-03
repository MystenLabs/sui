// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::prover_tests;

use sui::dynamic_field;

public struct Obj has key, store {
    id: UID,
}

// ====================================================================
// Object ownership
// ====================================================================

public fun simple_transfer(o: Obj, recipient: address) {
    transfer::public_transfer(o, recipient);
}

public fun simple_share(o: Obj) {
    transfer::public_share_object(o)
}

public fun simple_freeze(o: Obj) {
    transfer::public_freeze_object(o)
}

public fun simple_delete(o: Obj) {
    let Obj { id } = o;
    id.delete();
}

// ====================================================================
// Dynamic fields
// ====================================================================

public fun simple_field_add(o: &mut Obj, n1: u64, v1: u8, n2: u8, v2: u64) {
    dynamic_field::add(&mut o.id, n1, v1);
    dynamic_field::add(&mut o.id, n2, v2);
}

public fun simple_field_remove(o: &mut Obj, n1: u64, n2: u8) {
    dynamic_field::remove<u64, u8>(&mut o.id, n1);
    dynamic_field::remove<u8, u64>(&mut o.id, n2);
}
