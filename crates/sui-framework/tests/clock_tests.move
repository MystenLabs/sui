// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::clock_tests {
    use std::vector;
    use sui::clock::{Self, Clock};
    use sui::test_scenario as ts;

    #[test]
    fun creating_a_clock_and_incrementing_it() {
        let ts = ts::begin(@0x1);
        let ctx = ts::ctx(&mut ts);
        clock::create_for_testing(ctx);

        let eff = ts::next_tx(&mut ts, @0x2);
        // Make sure the clock was created
        assert!(vector::length(&ts::shared(&eff)) == 1, 0);

        // ...and that we can fetch it and update it
        let clock = ts::take_shared<Clock>(&ts);
        clock::increment_for_testing(&mut clock, 42);
        ts::return_shared(clock);

        ts::next_tx(&mut ts, @0x3);
        let clock = ts::take_shared<Clock>(&ts);
        // ...and read the updates
        assert!(clock::timestamp_ms(&clock) == 42, 1);
        ts::return_shared(clock);
        ts::end(ts);
    }
}
