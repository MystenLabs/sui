// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_test_code::init_with_object;

public struct MyObject has key, store {
    id: UID,
    value: u64,
}

fun init(ctx: &mut TxContext) {
    let obj = MyObject {
        id: object::new(ctx),
        value: 42,
    };
    transfer::public_transfer(obj, ctx.sender());
}
