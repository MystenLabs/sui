// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::object {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Object has key {
        id: UID
    }

    /// If function is defined as public - any module can call it.
    /// Non-entry functions are also allowed to have return values.
    public fun create(ctx: &mut TxContext): Object {
        Object { id: object::new(ctx) }
    }

    /// Entrypoints can't have return values as they can only be called
    /// directly in a transaction and the returned value can't be used.
    /// However, `entry` without `public` disallows calling this method from
    /// other Move modules.
    entry fun create_and_transfer(to: address, ctx: &mut TxContext) {
        transfer::transfer(create(ctx), to)
    }
}
