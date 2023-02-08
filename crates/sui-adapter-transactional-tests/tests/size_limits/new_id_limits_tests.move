// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limts on number of created IDs 

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
//# run Test::M1::create_n_ids --args 1

// create at create count limit should succeed
//# run Test::M1::create_n_ids --args 256

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 257

// create above create count limit should fail
//# run Test::M1::create_n_ids --args 300
