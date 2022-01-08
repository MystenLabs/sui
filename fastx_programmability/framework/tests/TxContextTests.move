#[test_only]
module FastX::TxContextTests {
    use FastX::TxContext;

    #[test]
    fun test_id_generation() {
        let ctx = TxContext::dummy();
        assert!(TxContext::get_ids_created(&ctx) == 0, 0);

        let id1 = TxContext::new_id(&mut ctx);
        let id2 = TxContext::new_id(&mut ctx);

        // new_id should always produce fresh ID's
        assert!(id1 != id2, 1);
        assert!(TxContext::get_ids_created(&ctx) == 2, 2);
    }

}