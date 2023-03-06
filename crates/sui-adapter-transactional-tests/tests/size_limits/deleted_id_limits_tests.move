// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limts on number of deleted IDs 

//# init --addresses Test=0x0

//# publish

/// Test deleted id limits enforced
/// Right now, we should never be able to hit the delete limit because we will hit the create limit first
module Test::M1 {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::vector;

    public entry fun delete_n_ids(n: u64, ctx: &mut TxContext) {
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

// delete below delete count limit should succeed
//# run Test::M1::delete_n_ids --args 1

// delete below delete count limit should succeed
//# run Test::M1::delete_n_ids --args 256

// delete at delete count limit should succeed
//# run Test::M1::delete_n_ids --args 2048

// delete above delete count limit should fail
//# run Test::M1::delete_n_ids --args 2049

// delete above delete count limit should fail
//# run Test::M1::delete_n_ids --args 4096
