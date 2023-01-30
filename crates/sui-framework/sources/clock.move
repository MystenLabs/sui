// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// APIs for accessing time from move calls, via the `Clock`: a unique
/// shared object that is created at 0x6 during genesis.
module sui::clock {
    use sui::object::{Self, UID};
    use sui::transfer;

    friend sui::genesis;

    struct Clock has key {
        id: UID,
        timestamp_ms: u64,
    }

    /// The `clock`'s current timestamp, in milliseconds.
    public fun timestamp_ms(clock: &Clock): u64 {
        clock.timestamp_ms
    }

    /// Create and share the singleton Clock -- this function is
    /// called exactly once, during genesis.
    public(friend) fun create() {
        transfer::share_object(Clock {
            id: object::clock(),
            // Initialised to zero, but set to a real timestamp by a
            // system transaction before it can be witnessed by a move
            // call.
            timestamp_ms: 0,
        })
    }

    #[test_only]
    public fun increment_for_testing(clock: &mut Clock, tick: u64) {
        clock.timestamp_ms = clock.timestamp_ms + tick;
    }
}
