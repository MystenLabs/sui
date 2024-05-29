// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::tx_context_tests {

    #[test]
    fun test_id_generation() {
        let mut ctx = tx_context::dummy();
        assert!(ctx.get_ids_created() == 0);

        let id1 = object::new(&mut ctx);
        let id2 = object::new(&mut ctx);

        // new_id should always produce fresh ID's
        assert!(&id1 != &id2);
        assert!(ctx.get_ids_created() == 2);
        id1.delete();
        id2.delete();
    }

}
