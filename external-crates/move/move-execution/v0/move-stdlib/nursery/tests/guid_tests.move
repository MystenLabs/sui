#[test_only]
module std::guid_tests {
    use std::guid;

    #[test(s = @0x42)]
    fun test_basics(s: signer) {
        let id1 = guid::create(&s);
        let id2 = guid::create(&s);

        assert!(&id1 != &id2, 0);

        assert!(guid::creator_address(&id1) == @0x42, 1);
        assert!(guid::creator_address(&id2) == @0x42, 2);

        assert!(guid::creation_num(&id1) == 0, 3);
        assert!(guid::creation_num(&id2) == 1, 4);

        assert!(guid::get_next_creation_num(@0x42) == 2, 5);
        assert!(guid::get_next_creation_num(@0x0) == 0, 6);
    }

    #[test(s = @0x42)]
    fun test_id(s: signer) {
        let guid = guid::create(&s);
        let id1 = guid::id(&guid);

        assert!(guid::id_creator_address(&id1) == guid::creator_address(&guid), 3);
        assert!(guid::id_creation_num(&id1) == guid::creation_num(&guid), 4);
        assert!(guid::eq_id(&guid, &id1), 2);

        let id2 = guid::create_id(@0x42, 0);
        assert!(&id1 == &id2, 0);

        let _ids_are_copyable = copy id1;
    }

    #[test(s = @0x42)]
    fun test_delegation(s: signer) {
        let create_cap = guid::gen_create_capability(&s);
        let guid = guid::create_with_capability(@0x42, &create_cap);
        assert!(guid::creator_address(&guid) == @0x42, 1);
    }
}
