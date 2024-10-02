// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::id_tests {
    const EIdBytesMismatch: u64 = 0;

    public struct Object has key {
        id: object::UID,
    }

    #[test]
    fun test_get_id() {
        let mut ctx = tx_context::dummy();
        let id = object::new(&mut ctx);
        let obj_id = id.to_inner();
        let obj = Object { id };
        assert!(object::id(&obj) == obj_id, EIdBytesMismatch);
        let Object { id } = obj;
        id.delete();
    }
}
