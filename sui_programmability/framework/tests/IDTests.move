#[test_only]
module FastX::IDTests {
    use FastX::ID;
    use FastX::TxContext;

    const ID_BYTES_MISMATCH: u64 = 0;

    struct Object has key {
        id: ID::VersionedID,
    }

    #[test]
    fun test_get_id() {
        let ctx = TxContext::dummy();
        let id = TxContext::new_id(&mut ctx);
        let id_bytes = *ID::get_inner(&id);
        let obj = Object { id };
        assert!(ID::get_inner(ID::get_id(&obj)) == &id_bytes, ID_BYTES_MISMATCH);
        let Object { id } = obj;
        ID::delete(id);
    }
}