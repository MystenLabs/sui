// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limits on number and sizes of emitted events

//# init --addresses Test=0x0

//# publish

/// Test event limits enforced
module Test::M1 {
    use sui::event;
    use sui::tx_context::TxContext;
    use std::vector;
    use sui::bcs;

    struct NewValueEvent has copy, drop {
        contents: vector<u8>
    }

    // create an object whose Move BCS representation is `n` bytes
    public fun create_object_with_size(n: u64): NewValueEvent {
        // minimum object size for NewValueEvent is 1 byte for vector length
        assert!(n > 1, 0);
        let contents = vector[];
        let i = 0;
        let bytes_to_add = n - 1;
        while (i < bytes_to_add) {
            vector::push_back(&mut contents, 9);
            i = i + 1;
        };
        let s = NewValueEvent { contents };
        let size = vector::length(&bcs::to_bytes(&s));
        // shrink by 1 byte until we match size. mismatch happens because of len(UID) + vector length byte
        while (size > n) {
            let _ = vector::pop_back(&mut s.contents);
            // hack: assume this doesn't change the size of the BCS length byte
            size = size - 1;
        };
        // double-check that we got the size right
        assert!(vector::length(&bcs::to_bytes(&s)) == n, 1);
        s
    }

    // Emit small (less than max size) events to test that the number of events is limited to the max count
    public entry fun emit_n_small_events(n: u64, _ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            event::emit(create_object_with_size(30));
            i = i + 1;
        };
    }
    // Emit object with roughly size `n`
    public entry fun emit_object_with_approx_size(n: u64) {
        event::emit(create_object_with_size(n));
    }
}
// Check count limits
// emit below event count limit should succeed
//# run Test::M1::emit_n_small_events --args 1 --gas-budget 1000000

// emit at event count limit should succeed
//# run Test::M1::emit_n_small_events --args 256 --gas-budget 2000000

// emit above event count limit should fail
//# run Test::M1::emit_n_small_events --args 257 --gas-budget 1000000

// emit above event count limit should fail
//# run Test::M1::emit_n_small_events --args 300 --gas-budget 1000000

// emit below event size limit should succeed
//# run Test::M1::emit_object_with_approx_size --args 200000 --gas-budget 2000000

// emit above event size limit should fail
//# run Test::M1::emit_object_with_approx_size --args 259000 --gas-budget 1000000
