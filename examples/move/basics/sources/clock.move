// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::clock {
    use sui::{clock::Clock, event};

    public struct TimeEvent has copy, drop, store {
        timestamp_ms: u64,
    }

    entry fun access(clock: &Clock) {
        event::emit(TimeEvent { timestamp_ms: clock.timestamp_ms() });
    }
}

module basics::clock_tests {
    #[test]
    fun creating_a_clock_and_incrementing_it() {
        let mut ctx = tx_context::dummy();
        let mut clock = sui::clock::create_for_testing(&mut ctx);

        clock.increment_for_testing(42);
        assert!(clock.timestamp_ms() == 42, 1);

        clock.set_for_testing(50);
        assert!(clock.timestamp_ms() == 50, 1);

        clock.destroy_for_testing();
    }
}