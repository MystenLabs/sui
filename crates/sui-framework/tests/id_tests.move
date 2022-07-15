// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::id_tests {
    use sui::object;
    use sui::tx_context;

    const ID_BYTES_MISMATCH: u64 = 0;

    struct Object has key {
        info: object::Info,
    }

    #[test]
    fun test_get_id() {
        let ctx = tx_context::dummy();
        let info = object::new(&mut ctx);
        let obj_id = *object::info_id(&info);
        let obj = Object { info };
        assert!(*object::id(&obj) == obj_id, ID_BYTES_MISMATCH);
        let Object { info } = obj;
        object::delete(info);
    }
}
