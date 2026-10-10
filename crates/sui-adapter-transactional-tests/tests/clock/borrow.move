// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `clock::borrow` reads the Clock without it being a transaction input: the
// timestamp follows the clock as it advances, agrees with an explicitly passed
// Clock, and is readable from a function written against `&Clock`.

//# init --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::m {
    use sui::clock::{Self, Clock};

    public struct Stamp has key, store {
        id: UID,
        timestamp_ms: u64,
    }

    // Written against `&Clock`, as existing code is.
    fun timestamp_of(clock: &Clock): u64 {
        clock.timestamp_ms()
    }

    public fun record(ctx: &mut TxContext) {
        let stamp = Stamp { id: object::new(ctx), timestamp_ms: timestamp_of(clock::borrow()) };
        transfer::public_transfer(stamp, ctx.sender())
    }

    public fun assert_matches(clock: &Clock) {
        assert!(clock::borrow().timestamp_ms() == clock.timestamp_ms(), 0);
    }

    public fun read_then_abort() {
        let _ = clock::borrow().timestamp_ms();
        abort 42
    }
}

//# run test::m::record --sender A

//# view-object 2,0

//# advance-clock --duration-ns 1500000000

//# run test::m::record --sender A

//# view-object 5,0

//# run test::m::assert_matches --args immshared(6) --sender A

//# run test::m::read_then_abort --sender A
