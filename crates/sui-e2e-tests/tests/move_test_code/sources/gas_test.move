// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::gas_test;

use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};

public struct TestObject has key {
    id: UID,
    value: u64,
    data: vector<u8>,
}

public entry fun create_object_with_storage(value: u64, ctx: &mut TxContext) {
    let obj = TestObject {
        id: object::new(ctx),
        value,
        data: vector[1, 2, 3, 4, 5],
    };
    transfer::transfer(obj, tx_context::sender(ctx));
}

public entry fun delete_object(obj: TestObject) {
    let TestObject { id, value: _, data: _ } = obj;
    object::delete(id);
}

public entry fun abort_with_computation(should_abort: bool) {
    let mut i = 0;
    while (i < 100) {
        i = i + 1;
    };

    if (should_abort) {
        abort 42
    }
}
