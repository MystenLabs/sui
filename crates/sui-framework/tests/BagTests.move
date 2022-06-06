// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::BagTests {
    use Sui::Bag::{Self, Bag};
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario;
    use Sui::TxContext;

    const EBAG_SIZE_MISMATCH: u64 = 0;
    const EOBJECT_NOT_FOUND: u64 = 1;

    struct Object1 has key, store {
        id: VersionedID,
    }

    struct Object2 has key, store {
        id: VersionedID,
    }

    #[test]
    public(script) fun test_bag() {
        let sender = @0x0;
        let scenario = &mut TestScenario::begin(&sender);

        // Create a new Bag and transfer it to the sender.
        TestScenario::next_tx(scenario, &sender);
        {
            Bag::create(TestScenario::ctx(scenario));
        };

        // Add two objects of different types into the bag.
        TestScenario::next_tx(scenario, &sender);
        {
            let bag = TestScenario::take_owned<Bag>(scenario);
            assert!(Bag::size(&bag) == 0, EBAG_SIZE_MISMATCH);

            let obj1 = Object1 { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id1 = *ID::id(&obj1);
            let obj2 = Object2 { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id2 = *ID::id(&obj2);

            Bag::add(&mut bag, obj1);
            Bag::add(&mut bag, obj2);
            assert!(Bag::size(&bag) == 2, EBAG_SIZE_MISMATCH);

            assert!(Bag::contains(&bag, &id1), EOBJECT_NOT_FOUND);
            assert!(Bag::contains(&bag, &id2), EOBJECT_NOT_FOUND);

            TestScenario::return_owned(scenario, bag);
        };
        // TODO: Test object removal once we can retrieve object owned objects from TestScenario.
    }

    #[test]
    #[expected_failure(abort_code = 520)]
    public(script) fun test_init_with_invalid_max_capacity() {
        let ctx = TxContext::dummy();
        // Sui::Bag::DEFAULT_MAX_CAPACITY is not readable outside the module
        let max_capacity = 65536;
        let bag = Bag::new_with_max_capacity(&mut ctx, max_capacity + 1);
        Bag::transfer_(bag, TxContext::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 520)]
    public(script) fun test_init_with_zero() {
        let ctx = TxContext::dummy();
        let bag = Bag::new_with_max_capacity(&mut ctx, 0);
        Bag::transfer_(bag, TxContext::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 776)]
    public(script) fun test_exceed_max_capacity() {
        let ctx = TxContext::dummy();
        let bag = Bag::new_with_max_capacity(&mut ctx, 1);

        let obj1 = Object1 { id: TxContext::new_id(&mut ctx) };
        Bag::add(&mut bag, obj1);
        let obj2 = Object2 { id: TxContext::new_id(&mut ctx) };
        Bag::add(&mut bag, obj2);
        Bag::transfer_(bag, TxContext::sender(&ctx));
    }
}
