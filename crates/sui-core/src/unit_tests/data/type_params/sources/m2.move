// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module type_params::m2 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;

    public struct AnotherObject has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create_and_transfer(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            AnotherObject { id: object::new(ctx), value },
            recipient
        )
    }

    public fun create(value: u64, ctx: &mut TxContext): AnotherObject {
        AnotherObject { id: object::new(ctx), value }
    }


}
