// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test limts on number of emitted events 

//# init --addresses Test=0x0

//# publish

/// Test event limits enforced
module Test::M1 {
    use sui::event;
    use sui::tx_context::TxContext;

    struct NewValueEvent has copy, drop {
        new_value: u64
    }

    // test that the number of events is limited to 

    public entry fun emit_n_events(n: u64, _ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            event::emit(NewValueEvent { new_value: i});
            i = i + 1;
        };
    }
}
// emit below event count limit should succeed
//# run Test::M1::emit_n_events --args 1

// emit at event count limit should succeed
//# run Test::M1::emit_n_events --args 256

// emit above event count limit should fail
//# run Test::M1::emit_n_events --args 257

// emit above event count limit should fail
//# run Test::M1::emit_n_events --args 300
