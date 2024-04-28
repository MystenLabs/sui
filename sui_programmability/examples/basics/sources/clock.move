// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example demonstrates reading a clock object.
/// Current time is emitted as an event in the get_time transaction
module basics::clock {
    use sui::{clock::Clock, event};

    public struct TimeEvent has copy, drop {
        timestamp_ms: u64,
    }

    /// Emit event with current time.
    entry fun access(clock: &Clock) {
        event::emit(TimeEvent { timestamp_ms: clock.timestamp_ms() });
    }
}
