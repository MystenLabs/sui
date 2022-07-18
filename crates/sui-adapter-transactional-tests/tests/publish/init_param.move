//# init --addresses Test=0x0
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# publish

// initializer not valid due extra non-ctx param

module Test::M1 {
    use sui::object::{Self, Info};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct Object has key, store {
        info: Info,
        value: u64,
    }

    // value param invalid
    fun init(ctx: &mut TxContext, value: u64) {
        let singleton = Object { info: object::new(ctx), value };
        transfer::transfer(singleton, tx_context::sender(ctx))
    }
}
