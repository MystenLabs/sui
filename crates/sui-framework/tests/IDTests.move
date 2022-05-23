// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module Sui::IDTests {
    use Sui::ID;
    use Sui::TxContext;

    const ID_BYTES_MISMATCH: u64 = 0;

    struct Object has key {
        id: ID::VersionedID,
    }

    #[test]
    fun test_get_id() {
        let ctx = TxContext::dummy();
        let versioned_id = TxContext::new_id(&mut ctx);
        let obj_id = *ID::inner(&versioned_id);
        let obj = Object { id: versioned_id };
        assert!(*ID::id(&obj) == obj_id, ID_BYTES_MISMATCH);
        let Object { id } = obj;
        ID::delete(id);
    }
}
