// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::collection_tests {
    use sui::bag::{Self, Bag};
    use sui::collection::{Self, Collection};
    use sui::object::{Self, UID};
    use sui::test_scenario;
    use sui::typed_id;
    use sui::tx_context;

    struct Object has key, store {
        id: UID,
    }

    #[test]
    fun test_collection() {
        let sender = @0x0;
        let scenario = &mut test_scenario::begin(&sender);

        // Create a new Collection and transfer it to the sender.
        test_scenario::next_tx(scenario, &sender);
        {
            collection::create<Object>(test_scenario::ctx(scenario));
        };

        // Add two objects of different types into the collection.
        test_scenario::next_tx(scenario, &sender);
        {
            let collection = test_scenario::take_owned<Collection<Object>>(scenario);
            assert!(collection::size(&collection) == 0, 0);

            let obj1 = Object { id: object::new(test_scenario::ctx(scenario)) };
            let obj2 = Object { id: object::new(test_scenario::ctx(scenario)) };

            let item_id1 = collection::add(&mut collection, obj1, test_scenario::ctx(scenario));
            let item_id2 = collection::add(&mut collection, obj2, test_scenario::ctx(scenario));
            assert!(collection::size(&collection) == 2, 0);

            assert!(collection::contains(&collection, typed_id::as_id(&item_id1)), 0);
            assert!(collection::contains(&collection, typed_id::as_id(&item_id2)), 0);

            test_scenario::return_owned(scenario, collection);
        };
    }

    #[test]
    fun test_collection_bag_interaction() {
        let sender = @0x0;
        let scenario = &mut test_scenario::begin(&sender);

        // Create a new Collection and a new Bag and transfer them to the sender.
        test_scenario::next_tx(scenario, &sender);
        {
            collection::create<Object>(test_scenario::ctx(scenario));
            bag::create(test_scenario::ctx(scenario));
        };

        // Add a new object to the Collection.
        test_scenario::next_tx(scenario, &sender);
        {
            let collection = test_scenario::take_owned<Collection<Object>>(scenario);
            let obj = Object { id: object::new(test_scenario::ctx(scenario)) };
            collection::add(&mut collection, obj, test_scenario::ctx(scenario));
            test_scenario::return_owned(scenario, collection);
        };

        // Remove the object from the collection and add it to the bag.
        test_scenario::next_tx(scenario, &sender);
        {
            let collection = test_scenario::take_owned<Collection<Object>>(scenario);
            let bag = test_scenario::take_owned<Bag>(scenario);
            let obj = test_scenario::take_child_object<Collection<Object>, collection::Item<Object>>(scenario, &collection);

            let obj = collection::remove(&mut collection, obj);
            let item_id = bag::add(&mut bag, obj, test_scenario::ctx(scenario));

            assert!(collection::size(&collection) == 0, 0);
            assert!(bag::size(&bag) == 1, 0);
            assert!(bag::contains(&bag, typed_id::as_id(&item_id)), 0);

            test_scenario::return_owned(scenario, collection);
            test_scenario::return_owned(scenario, bag);
        };

        // Remove the object from the bag and add it back to the collection.
        test_scenario::next_tx(scenario, &sender);
        {
            let collection = test_scenario::take_owned<Collection<Object>>(scenario);
            let bag = test_scenario::take_owned<Bag>(scenario);
            let obj = test_scenario::take_child_object<Bag, bag::Item<Object>>(scenario, &bag);

            let obj = bag::remove(&mut bag, obj);
            let item_id = collection::add(&mut collection, obj, test_scenario::ctx(scenario));

            assert!(collection::size(&collection) == 1, 0);
            assert!(bag::size(&bag) == 0, 0);
            assert!(collection::contains(&collection, typed_id::as_id(&item_id)), 0);

            test_scenario::return_owned(scenario, collection);
            test_scenario::return_owned(scenario, bag);
        };

    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_init_with_invalid_max_capacity() {
        let ctx = tx_context::dummy();
        // Sui::collection::DEFAULT_MAX_CAPACITY is not readable outside the module
        let max_capacity = 65536;
        let collection = collection::new_with_max_capacity<Object>(&mut ctx, max_capacity + 1);
        collection::transfer(collection, tx_context::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_init_with_zero() {
        let ctx = tx_context::dummy();
        let collection = collection::new_with_max_capacity<Object>(&mut ctx, 0);
        collection::transfer(collection, tx_context::sender(&ctx));
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_exceed_max_capacity() {
        let ctx = tx_context::dummy();
        let collection = collection::new_with_max_capacity<Object>(&mut ctx, 1);

        let obj1 = Object { id: object::new(&mut ctx) };
        collection::add(&mut collection, obj1, &mut ctx);
        let obj2 = Object { id: object::new(&mut ctx) };
        collection::add(&mut collection, obj2, &mut ctx);
        collection::transfer(collection, tx_context::sender(&ctx));
    }
}
