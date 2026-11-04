// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::gas_test;

use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};
use sui::event;

public struct TestObject has key {
    id: UID,
    value: u64,
    data: vector<u8>,
}

public struct UserEvent has copy, drop {
    sequence: u64,
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

public entry fun transfer_object(obj: TestObject, recipient: address) {
    transfer::transfer(obj, recipient);
}

public entry fun create_object_with_large_storage(
    value: u64,
    data: vector<u8>,
    ctx: &mut TxContext,
) {
    let obj = TestObject {
        id: object::new(ctx),
        value,
        data,
    };
    transfer::transfer(obj, tx_context::sender(ctx));
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

public entry fun mutate_and_abort(obj: &mut TestObject) {
    obj.value = obj.value + 1;
    abort 42
}

public entry fun mutate_two_and_abort(first: &mut TestObject, second: &mut TestObject) {
    first.value = first.value + 1;
    second.value = second.value + 1;
    abort 42
}

public entry fun resize_object(obj: &mut TestObject, data: vector<u8>) {
    obj.data = data;
}

public entry fun emit_many(count: u64) {
    let mut sequence = 0;
    while (sequence < count) {
        event::emit(UserEvent { sequence });
        sequence = sequence + 1;
    }
}
