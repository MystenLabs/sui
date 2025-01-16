// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example demonstrates a basic use of a shared object.
/// Rules:
/// - anyone can create and share a counter
/// - everyone can increment a counter by 1
/// - the owner of the counter can reset it to any value
module basics::counter {
    /// A shared counter.
    public struct Counter has key {
        id: UID,
        owner: address,
        value: u64,
    }

    public fun owner(counter: &Counter): address {
        counter.owner
    }

    public fun value(counter: &Counter): u64 {
        counter.value
    }

    /// Create and share a Counter object.
    public fun create(ctx: &mut TxContext) {
        transfer::share_object(Counter {
            id: object::new(ctx),
            owner: tx_context::sender(ctx),
            value: 0,
        })
    }

    /// Increment a counter by 1.
    public fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }

    /// Set value (only runnable by the Counter owner)
    public fun set_value(counter: &mut Counter, value: u64, ctx: &TxContext) {
        assert!(counter.owner == ctx.sender(), 0);
        counter.value = value;
    }

    /// Assert a value for the counter.
    public fun assert_value(counter: &Counter, value: u64) {
        assert!(counter.value == value, 0)
    }

    /// Delete counter (only runnable by the Counter owner)
    public fun delete(counter: Counter, ctx: &TxContext) {
        assert!(counter.owner == ctx.sender(), 0);
        let Counter { id, owner: _, value: _ } = counter;
        id.delete();
    }
}

#[test_only]
module basics::counter_test {
    use basics::counter::{Self, Counter};
    use sui::test_scenario as ts;

    #[test]
    fun test_counter() {
        let owner = @0xC0FFEE;
        let user1 = @0xA1;

        let mut ts = ts::begin(user1);

        {
            ts.next_tx(owner);
            counter::create(ts.ctx());
        };

        {
            ts.next_tx(user1);
            let mut counter: Counter = ts.take_shared();

            assert!(counter.owner() == owner);
            assert!(counter.value() == 0);

            counter.increment();
            counter.increment();
            counter.increment();

            ts::return_shared(counter);
        };

        {
            ts.next_tx(owner);
            let mut counter: Counter = ts.take_shared();

            assert!(counter.owner() == owner);
            assert!(counter.value() == 3);

            counter.set_value(100, ts.ctx());

            ts::return_shared(counter);
        };

        {
            ts.next_tx(user1);
            let mut counter: Counter = ts.take_shared();

            assert!(counter.owner() == owner);
            assert!(counter.value() == 100);

            counter.increment();
            assert!(counter.value() == 101);

            ts::return_shared(counter);
        };

        ts.end();
    }
}
