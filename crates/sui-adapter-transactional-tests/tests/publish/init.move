// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
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
        let value = 42;
        let singleton = Object { id: TxContext::new_id(ctx), value };
        Transfer::transfer(singleton, TxContext::sender(ctx))
    }
}

//# view-object 104

//# view-object 103
