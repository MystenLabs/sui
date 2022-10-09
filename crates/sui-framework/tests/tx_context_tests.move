// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::tx_context_tests {
    use sui::object;
    use sui::tx_context;

    #[test]
    fun test_id_generation() {
        let ctx = tx_context::dummy();
        assert!(tx_context::get_ids_created(&ctx) == 0, 0);

        let id1 = object::new(&mut ctx);
        let id2 = object::new(&mut ctx);

        // new_id should always produce fresh ID's
        assert!(&id1 != &id2, 1);
        assert!(tx_context::get_ids_created(&ctx) == 2, 2);
        object::delete(id1);
        object::delete(id2);
    }

}
