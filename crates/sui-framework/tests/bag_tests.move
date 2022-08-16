// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::bag_tests {
    use sui::bag::{Self, Bag};
    use sui::object::{Self, UID};
    use sui::test_scenario;
    use sui::typed_id;
    use sui::tx_context;

    const EBAG_SIZE_MISMATCH: u64 = 0;
    const EOBJECT_NOT_FOUND: u64 = 1;

    struct Object1 has key, store {
        id: UID,
    }

    struct Object2 has key, store {
        id: UID,
    }

    #[test]
    fun test_bag() {
        let sender = @0x0;
        let scenario = &mut test_scenario::begin(&sender);

        // Create a new Bag and transfer it to the sender.
        test_scenario::next_tx(scenario, &sender);
        {
            bag::create(test_scenario::ctx(scenario));
        };

        // Add two objects of different types into the bag.
        test_scenario::next_tx(scenario, &sender);
        {
            let bag = test_scenario::take_owned<Bag>(scenario);
            assert!(bag::size(&bag) == 0, EBAG_SIZE_MISMATCH);

            let obj1 = Object1 { id: object::new(test_scenario::ctx(scenario)) };
            let obj2 = Object2 { id: object::new(test_scenario::ctx(scenario)) };

            let item_id1 = bag::add(&mut bag, obj1, test_scenario::ctx(scenario));
            let item_id2 = bag::add(&mut bag, obj2, test_scenario::ctx(scenario));
            assert!(bag::size(&bag) == 2, EBAG_SIZE_MISMATCH);

            assert!(bag::contains(&bag, typed_id::as_id(&item_id1)), EOBJECT_NOT_FOUND);
            assert!(bag::contains(&bag, typed_id::as_id(&item_id2)), EOBJECT_NOT_FOUND);

            test_scenario::return_owned(scenario, bag);
        };
        // TODO: Test object removal once we can retrieve object owned objects from test_scenario.
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_init_with_invalid_max_capacity() {
        let ctx = tx_context::dummy();
        // Sui::bag::DEFAULT_MAX_CAPACITY is not readable outside the module
        let max_capacity = 65536;
        let bag = bag::new_with_max_capacity(&mut ctx, max_capacity + 1);
        bag::transfer(bag, tx_context::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_init_with_zero() {
        let ctx = tx_context::dummy();
        let bag = bag::new_with_max_capacity(&mut ctx, 0);
        bag::transfer(bag, tx_context::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 2)]
    fun test_exceed_max_capacity() {
        let ctx = tx_context::dummy();
        let bag = bag::new_with_max_capacity(&mut ctx, 1);

        let obj1 = Object1 { id: object::new(&mut ctx) };
        bag::add(&mut bag, obj1, &mut ctx);
        let obj2 = Object2 { id: object::new(&mut ctx) };
        bag::add(&mut bag, obj2, &mut ctx);
        bag::transfer(bag, tx_context::sender(&ctx));
    }
}
