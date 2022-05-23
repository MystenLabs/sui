// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::CollectionTests {
    use Sui::Bag::{Self, Bag};
    use Sui::Collection::{Self, Collection};
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario;
    use Sui::TxContext;

    struct Object has key {
        id: VersionedID,
    }

    #[test]
    public(script) fun test_collection() {
        let sender = @0x0;
        let scenario = &mut TestScenario::begin(&sender);

        // Create a new Collection and transfer it to the sender.
        TestScenario::next_tx(scenario, &sender);
        {
            Collection::create<Object>(TestScenario::ctx(scenario));
        };

        // Add two objects of different types into the collection.
        TestScenario::next_tx(scenario, &sender);
        {
            let collection = TestScenario::take_owned<Collection<Object>>(scenario);
            assert!(Collection::size(&collection) == 0, 0);

            let obj1 = Object { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id1 = *ID::id(&obj1);
            let obj2 = Object { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id2 = *ID::id(&obj2);

            Collection::add(&mut collection, obj1);
            Collection::add(&mut collection, obj2);
            assert!(Collection::size(&collection) == 2, 0);

            assert!(Collection::contains(&collection, &id1), 0);
            assert!(Collection::contains(&collection, &id2), 0);

            TestScenario::return_owned(scenario, collection);
        };
    }

    #[test]
    public(script) fun test_collection_bag_interaction() {
        let sender = @0x0;
        let scenario = &mut TestScenario::begin(&sender);

        // Create a new Collection and a new Bag and transfer them to the sender.
        TestScenario::next_tx(scenario, &sender);
        {
            Collection::create<Object>(TestScenario::ctx(scenario));
            Bag::create(TestScenario::ctx(scenario));
        };

        // Add a new object to the Collection.
        TestScenario::next_tx(scenario, &sender);
        {
            let collection = TestScenario::take_owned<Collection<Object>>(scenario);
            let obj = Object { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            Collection::add(&mut collection, obj);
            TestScenario::return_owned(scenario, collection);
        };

        // Remove the object from the collection and add it to the bag.
        TestScenario::next_tx(scenario, &sender);
        {
            let collection = TestScenario::take_owned<Collection<Object>>(scenario);
            let bag = TestScenario::take_owned<Bag>(scenario);
            let obj = TestScenario::take_child_object<Collection<Object>, Object>(scenario, &collection);
            let id = *ID::id(&obj);

            let (obj, child_ref) = Collection::remove(&mut collection, obj);
            Bag::add_child_object(&mut bag, obj, child_ref);

            assert!(Collection::size(&collection) == 0, 0);
            assert!(Bag::size(&bag) == 1, 0);
            assert!(Bag::contains(&bag, &id), 0);

            TestScenario::return_owned(scenario, collection);
            TestScenario::return_owned(scenario, bag);
        };

        // Remove the object from the bag and add it back to the collection.
        TestScenario::next_tx(scenario, &sender);
        {
            let collection = TestScenario::take_owned<Collection<Object>>(scenario);
            let bag = TestScenario::take_owned<Bag>(scenario);
            let obj = TestScenario::take_child_object<Bag, Object>(scenario, &bag);
            let id = *ID::id(&obj);

            let obj = Bag::remove(&mut bag, obj);
            Collection::add(&mut collection, obj);

            assert!(Collection::size(&collection) == 1, 0);
            assert!(Bag::size(&bag) == 0, 0);
            assert!(Collection::contains(&collection, &id), 0);

            TestScenario::return_owned(scenario, collection);
            TestScenario::return_owned(scenario, bag);
        };

    }

    #[test]
    #[expected_failure(abort_code = 520)]
    public(script) fun test_init_with_invalid_max_capacity() {
        let ctx = TxContext::dummy();
        // Sui::Collection::DEFAULT_MAX_CAPACITY is not readable outside the module
        let max_capacity = 65536;
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, max_capacity + 1);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }

    #[test]
    #[expected_failure(abort_code = 520)]
    public(script) fun test_init_with_zero() {
        let ctx = TxContext::dummy();
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, 0);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }

    #[test]
    #[expected_failure(abort_code = 776)]
    public(script) fun test_exceed_max_capacity() {
        let ctx = TxContext::dummy();
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, 1);

        let obj1 = Object { id: TxContext::new_id(&mut ctx) };
        Collection::add(&mut collection, obj1);
        let obj2 = Object { id: TxContext::new_id(&mut ctx) };
        Collection::add(&mut collection, obj2);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }
}
