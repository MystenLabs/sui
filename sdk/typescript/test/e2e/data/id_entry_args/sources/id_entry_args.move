// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module id_entry_args::test {
    use sui::tx_context::TxContext;
    use sui::object::{Self, ID, UID};

    public entry fun test_id(id: ID, _ctx: &mut TxContext) {
        assert!(object::id_to_address(&id) == @0xc2b5625c221264078310a084df0a3137956d20ee, 0);
    }

    public entry fun test_uid(_ctx: &mut TxContext) {
        let uid = object::new(_ctx);
        let id = object::uid_to_inner(&uid);
        assert!(object::uid_to_address(&uid) == object::id_to_address(&id), 0);
        object::delete(uid);
    }
}
