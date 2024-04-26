// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module type_params::m1 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;
    use type_params::m2;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public struct GenObject<T: key + store> has key, store {
        id: UID,
        o: T,
    }

    public entry fun create_and_transfer(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun create_and_transfer_gen(value: u64, recipient: address, ctx: &mut TxContext) {
        let another = m2::create(value, ctx);
        transfer::public_transfer(
            GenObject { id: object::new(ctx), o: another },
            recipient
        )
    }

    public entry fun transfer_object<T: key + store>(o: T, recipient: address) {
        transfer::public_transfer(o, recipient);
    }


}
