// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limits on length of vectors

//# init --addresses Test=0x0

//# publish

/// Test vector length limits enforced
module Test::M1 {
    use std::vector;

    public entry fun push_n_items(n: u64) {
        let v: vector<u64> = vector::empty();
        let i = 0;
        while (i < n) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };
        i = 0;
        while (i < n) {
            let _ = vector::pop_back(&mut v);
            i = i + 1;
        };
        vector::destroy_empty(v);
    }
}

// push below ven len limit should succeed
//# run Test::M1::push_n_items --args 1 --uncharged

// push below vec len limit should succeed
//# run Test::M1::push_n_items --args 256 --uncharged

// run at vec len limit should succeed
//# run Test::M1::push_n_items --args 262144 --uncharged

// run above vec len limit should fail
//# run Test::M1::push_n_items --args 262145 --uncharged

// Check if we run out of gas before hitting limits

// run at vec len limit should succeed
//# run Test::M1::push_n_items --args 262144 --gas-budget 1000000000

// run above vec len limit should fail
//# run Test::M1::push_n_items --args 262145 --gas-budget 1000000000
