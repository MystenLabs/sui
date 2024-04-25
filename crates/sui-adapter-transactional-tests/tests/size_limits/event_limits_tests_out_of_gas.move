// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limits on number and sizes of emitted events

//# init --addresses Test=0x0

//# publish

/// Test event limits enforced
module Test::M1 {
    use sui::event;
    use sui::bcs;

    public struct NewValueEvent has copy, drop {
        contents: vector<u8>
    }

    // emit an event of size n
    public fun emit_event_with_size(mut n: u64) {
        // 46 seems to be the added size from event size derivation for `NewValueEvent`
        assert!(n > 46, 0);
        n = n - 46;
        // minimum object size for NewValueEvent is 1 byte for vector length
        assert!(n > 1, 0);
        let mut contents = vector[];
        let mut i = 0;
        let bytes_to_add = n - 1;
        while (i < bytes_to_add) {
            vector::push_back(&mut contents, 9);
            i = i + 1;
        };
        let mut s = NewValueEvent { contents };
        let mut size = vector::length(&bcs::to_bytes(&s));
        // shrink by 1 byte until we match size. mismatch happens because of len(UID) + vector length byte
        while (size > n) {
            let _ = vector::pop_back(&mut s.contents);
            // hack: assume this doesn't change the size of the BCS length byte
            size = size - 1;
        };

        event::emit(s);
    }

    // Emit small (less than max size) events to test that the number of events is limited to the max count
    public entry fun emit_n_small_events(n: u64, _ctx: &mut TxContext) {
        let mut i = 0;
        while (i < n) {
            emit_event_with_size(50);
            i = i + 1;
        };
    }

    // emit n events each of size s
    public fun emit_n_events_with_size(n: u64, s: u64) {
        let mut i = 0;
        while (i < n) {
            emit_event_with_size(s);
            i = i + 1;
        };
    }
}

// Check if we run out of gas before hitting limits

// Can't emit above event count limit without running out of gas
//# run Test::M1::emit_n_small_events --args 1025 --gas-budget 1000000000

// emit above event count limit should fail
//# run Test::M1::emit_n_small_events --args 2093 --gas-budget 1000000000

// emit above event size limit should fail
//# run Test::M1::emit_event_with_size --args 259000 --gas-budget 1000000000

// emit below total event size limit but above gas limit should fail
//# run Test::M1::emit_n_events_with_size --args 3 256000 --gas-budget 1000000000 --summarize
