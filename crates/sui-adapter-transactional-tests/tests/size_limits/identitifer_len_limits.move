// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test Identifer length limits

//# init --addresses Test=0x0

//# publish

/// Test Identifer length limits enforced for module name
module Test::M1_1234567891234567890123456789012345678912345678901234567890123456789123456789012345678908901234567891234567890123456789078912345678901234567890 {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::vector;

    public entry fun create_n_idscreate_n_idscreate_n_() {
    }
}

//# publish

/// Test Identifer length limits enforced for function name
module Test::M1_12345678912345678901234567890 {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::vector;

    public entry fun create_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_idscreate_n_ids() {
    }
}


//# publish

/// Test normal Identifer lengths
module Test::M1_1234567891234567890123456789012345678912345678901234567 {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::vector;

    public entry fun create_n_(n: u64, ctx: &mut TxContext) {
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
