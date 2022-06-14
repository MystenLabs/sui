// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Test::M1 {
    use sui::ID::VersionedID;
    use sui::TxContext::{Self, TxContext};
    use sui::Transfer;

    struct Object has key, store {
        id: VersionedID,
        value: u64,
    }

    // initializer that should be executed upon publishing this module
    fun init(ctx: &mut TxContext) {
        let singleton = Object { id: TxContext::new_id(ctx), value: 12 };
        Transfer::transfer(singleton, TxContext::sender(ctx))
    }
}
