// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::TxContextTests {
    use Sui::ID;
    use Sui::TxContext;

    #[test]
    fun test_id_generation() {
        let ctx = TxContext::dummy();
        assert!(TxContext::get_ids_created(&ctx) == 0, 0);

        let id1 = TxContext::new_id(&mut ctx);
        let id2 = TxContext::new_id(&mut ctx);

        // new_id should always produce fresh ID's
        assert!(&id1 != &id2, 1);
        assert!(TxContext::get_ids_created(&ctx) == 2, 2);
        ID::delete(id1);
        ID::delete(id2);
    }

}
