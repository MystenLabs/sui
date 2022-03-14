#[test_only]
module Sui::CollectionTests {
    use Sui::Collection::{Self, Collection};
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario;
    use Sui::TxContext;

    const ECOLLECTION_SIZE_MISMATCH: u64 = 0;
    const EOBJECT_NOT_FOUND: u64 = 1;

    struct Object has key {
        id: VersionedID,
    }

    #[test]
    fun test_collection() {
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
            let collection = TestScenario::remove_object<Collection<Object>>(scenario);
            assert!(Collection::size(&collection) == 0, ECOLLECTION_SIZE_MISMATCH);

            let obj1 = Object { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id1 = *ID::id(&obj1);
            let obj2 = Object { id: TxContext::new_id(TestScenario::ctx(scenario)) };
            let id2 = *ID::id(&obj2);

            Collection::add(&mut collection, obj1);
            Collection::add(&mut collection, obj2);
            assert!(Collection::size(&collection) == 2, ECOLLECTION_SIZE_MISMATCH);

            assert!(Collection::contains(&collection, &id1), EOBJECT_NOT_FOUND);
            assert!(Collection::contains(&collection, &id2), EOBJECT_NOT_FOUND);

            TestScenario::return_object(scenario, collection);
        };
        // TODO: Test object removal once we can retrieve object owned objects from TestScenario.
    }

    #[test]
    #[expected_failure(abort_code = 520)]
    fun test_init_with_invalid_max_capacity() {
        let ctx = TxContext::dummy();
        // Sui::Collection::DEFAULT_MAX_CAPACITY is not readable outside the module
        let max_capacity = 65536;
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, max_capacity + 1);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }

    #[test]
    #[expected_failure(abort_code = 520)]
    fun test_init_with_zero() {
        let ctx = TxContext::dummy();
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, 0);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }

    #[test]
    #[expected_failure(abort_code = 776)]
    fun test_exceed_max_capacity() {
        let ctx = TxContext::dummy();
        let collection = Collection::new_with_max_capacity<Object>(&mut ctx, 1);

        let obj1 = Object { id: TxContext::new_id(&mut ctx) };
        Collection::add(&mut collection, obj1);
        let obj2 = Object { id: TxContext::new_id(&mut ctx) };
        Collection::add(&mut collection, obj2);
        Collection::transfer(collection, TxContext::sender(&ctx), &mut ctx);
    }
}
