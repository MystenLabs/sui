// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::types_tests {
    use sui::types::type_tag_bytes;
    use sui::bag;
    use sui::tx_context;

    struct Foo<phantom T1, phantom T2> has store {}
    struct Bar {}
    struct Baz {}

    const E_TEST_FAILED: u64 = 1;

    #[test]
    fun test_type_tag_bytes() {
        // test hash properties
        let k1 = type_tag_bytes<Foo<Bar, Baz>>();
        let k2 = type_tag_bytes<Foo<Bar, Baz>>();
        let k3 = type_tag_bytes<Foo<Baz, Bar>>();

        assert!(k1 == k2, E_TEST_FAILED);
        assert!(k1 != k3, E_TEST_FAILED);

        // test using hash for bag key
        let ctx = tx_context::dummy();
        let b = bag::new(&mut ctx);
        let v1 = Foo<Bar, Baz> {};
        bag::add(&mut b, k1, v1);
        assert!(bag::contains(&b, k1), E_TEST_FAILED);
        let v1 = bag::remove(&mut b, k1);
        Foo<Bar, Baz> {} = v1;
        bag::destroy_empty(b);
    }
}
