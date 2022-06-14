// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::id_tests {
    use sui::id;
    use sui::tx_context;

    const ID_BYTES_MISMATCH: u64 = 0;

    struct Object has key {
        id: id::VersionedID,
    }

    #[test]
    fun test_get_id() {
        let ctx = tx_context::dummy();
        let versioned_id = tx_context::new_id(&mut ctx);
        let obj_id = *id::inner(&versioned_id);
        let obj = Object { id: versioned_id };
        assert!(*id::id(&obj) == obj_id, ID_BYTES_MISMATCH);
        let Object { id } = obj;
        id::delete(id);
    }
}
