// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limits on number of created IDs

//# init --addresses Test=0x0

//# publish

/// Test create id limits enforced
module Test::M1 {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::vector;

    public entry fun create_n_ids(n: u64, ctx: &mut TxContext) {
        let v: vector<UID> = vector::empty();
        let i = 0;
        while (i < n) {
            let id = object::new(ctx);
            vector::push_back(&mut v, id);
            i = i + 1;
        };
        i = 0;
        while (i < n) {
            let id = vector::pop_back(&mut v);
            object::delete(id);
            i = i + 1;
        };
        vector::destroy_empty(v);
    }
}

// create below create count limit should succeed
//# run Test::M1::create_n_ids --args 1 --uncharged

// create below create count limit should succeed
//# run Test::M1::create_n_ids --args 256 --uncharged

// create at create count limit should succeed
//# run Test::M1::create_n_ids --args 2048 --uncharged

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 2049 --uncharged

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 4096 --uncharged

// Check if we run out of gas before hitting limits

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 2049 --gas-budget 1000000000

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 4096 --gas-budget 1000000000
