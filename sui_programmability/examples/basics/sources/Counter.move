// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example demonstrates a basic use of a shared object.
/// Rules:
/// - anyone can create and share a counter
/// - everyone can increment a counter by 1
/// - the owner of the counter can reset it to any value
module Basics::Counter {
    use Sui::Transfer;
    use Sui::ID::VersionedID;
    use Sui::TxContext::{Self, TxContext};

    /// A shared counter.
    struct Counter has key {
        id: VersionedID,
        owner: address,
        value: u64
    }

    public fun owner(counter: &Counter): address {
        counter.owner
    }

    public fun value(counter: &Counter): u64 {
        counter.value
    }

    /// Create and share a Counter object.
    public(script) fun create(ctx: &mut TxContext) {
        Transfer::share_object(Counter {
            id: TxContext::new_id(ctx),
            owner: TxContext::sender(ctx),
            value: 0
        })
    }

    /// Increment a counter by 1.
    public(script) fun increment(counter: &mut Counter, _ctx: &mut TxContext) {
        counter.value = counter.value + 1;
    }

    /// Set value (only runnable by the Counter owner)
    public(script) fun set_value(counter: &mut Counter, value: u64, ctx: &mut TxContext) {
        assert!(counter.owner == TxContext::sender(ctx), 0);
        counter.value = value;
    }

    /// Assert a value for the counter.
    public(script) fun assert_value(counter: &Counter, value: u64, _ctx: &mut TxContext) {
        assert!(counter.value == value, 0)
    }
}

#[test_only]
module Basics::CounterTest {
    use Sui::TestScenario;
    use Basics::Counter;

    #[test]
    public(script) fun test_counter() {
        let owner = @0xC0FFEE;
        let user1 = @0xA1;

        let scenario = &mut TestScenario::begin(&user1);

        TestScenario::next_tx(scenario, &owner);
        {
            Counter::create(TestScenario::ctx(scenario));
        };

        TestScenario::next_tx(scenario, &user1);
        {
            let counter_wrapper = TestScenario::take_shared<Counter::Counter>(scenario);
            let counter = TestScenario::borrow_mut(&mut counter_wrapper);

            assert!(Counter::owner(counter) == owner, 0);
            assert!(Counter::value(counter) == 0, 1);

            Counter::increment(counter, TestScenario::ctx(scenario));
            Counter::increment(counter, TestScenario::ctx(scenario));
            Counter::increment(counter, TestScenario::ctx(scenario));
            TestScenario::return_shared(scenario, counter_wrapper);
        };

        TestScenario::next_tx(scenario, &owner);
        {
            let counter_wrapper = TestScenario::take_shared<Counter::Counter>(scenario);
            let counter = TestScenario::borrow_mut(&mut counter_wrapper);

            assert!(Counter::owner(counter) == owner, 0);
            assert!(Counter::value(counter) == 3, 1);

            Counter::set_value(counter, 100, TestScenario::ctx(scenario));

            TestScenario::return_shared(scenario, counter_wrapper);
        };

        TestScenario::next_tx(scenario, &user1);
        {
            let counter_wrapper = TestScenario::take_shared<Counter::Counter>(scenario);
            let counter = TestScenario::borrow_mut(&mut counter_wrapper);

            assert!(Counter::owner(counter) == owner, 0);
            assert!(Counter::value(counter) == 100, 1);

            Counter::increment(counter, TestScenario::ctx(scenario));

            assert!(Counter::value(counter) == 101, 2);

            TestScenario::return_shared(scenario, counter_wrapper);
        };
    }
}
