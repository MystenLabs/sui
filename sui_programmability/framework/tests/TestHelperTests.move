#[test_only]
module FastX::TestHelperTests {
    use FastX::ID;
    use FastX::TestHelper;
    use FastX::Transfer;
    use FastX::TxContext;

    const ID_BYTES_MISMATCH: u64 = 0;
    const VALUE_MISMATCH: u64 = 1;

    struct Object has key {
        id: ID::ID,
        value: u64,
    }

    #[test]
    fun test_transfer() {
        let ctx = TxContext::dummy();
        let id = TxContext::new_id(&mut ctx);
        let id_bytes = *ID::get_inner(&id);
        let obj = Object { id, value: 100 };
        Transfer::transfer(obj, TxContext::get_signer_address(&ctx));

        let received_obj = TestHelper::get_last_received_object(&ctx);
        let Object { id: received_id, value } = received_obj;
        assert!(ID::get_inner(&received_id) == &id_bytes, ID_BYTES_MISMATCH);
        assert!(value == 100, VALUE_MISMATCH);
        ID::delete(received_id);
    }
}