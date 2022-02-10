#[test_only]
module FastX::CollectionTests {
    use FastX::Collection;
    use FastX::ID::{Self, ID};
    use FastX::TxContext;

    const COLLECTION_SIZE_MISMATCH: u64 = 0;
    const OBJECT_NOT_FOUND: u64 = 1;

    struct Object has key {
        id: ID,
    }

    #[test]
    fun test_collection_add() {
        let ctx = TxContext::dummy();
        let collection = Collection::new(&mut ctx);
        assert!(Collection::size(&collection) == 0, COLLECTION_SIZE_MISMATCH);

        let obj1 = Object { id: TxContext::new_id(&mut ctx) };
        let id_bytes1 = *ID::get_id_bytes(&obj1);
        let obj2 = Object { id: TxContext::new_id(&mut ctx) };
        let id_bytes2 = *ID::get_id_bytes(&obj2);

        Collection::add(&mut collection, obj1);
        Collection::add(&mut collection, obj2);
        assert!(Collection::size(&collection) == 2, COLLECTION_SIZE_MISMATCH);

        assert!(Collection::contains(&collection, &id_bytes1), OBJECT_NOT_FOUND);
        assert!(Collection::contains(&collection, &id_bytes2), OBJECT_NOT_FOUND);

        Collection::transfer(collection, TxContext::get_signer_address(&ctx));
    }
}