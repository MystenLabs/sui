//# init --addresses Test=0x0
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# publish

// initializer not valid due to return value

module Test::M1 {
    use sui::object::{Self, Info};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct Object has key, store {
        info: Info,
        value: u64,
    }

    // initializer that should be executed upon publishing this module
    fun init(ctx: &mut TxContext): u64 {
        let value = 42;
        let singleton = Object { info: object::new(ctx), value };
        transfer::transfer(singleton, tx_context::sender(ctx));
        value
    }
}
