// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Reads the shared `Clock` (0x6) so transactions take it as an immutable shared
/// input.
module move_building_blocks::clocks {
    use sui::clock::{Self, Clock};

    public struct TimeStamped has key, store {
        id: UID,
        ts: u64,
    }

    public fun stamp(clock: &Clock, ctx: &mut TxContext) {
        let stamped = TimeStamped { id: object::new(ctx), ts: clock.timestamp_ms() };
        transfer::transfer(stamped, ctx.sender());
    }

    public fun update_stamp(stamped: &mut TimeStamped, clock: &Clock) {
        stamped.ts = clock::timestamp_ms(clock);
    }
}
