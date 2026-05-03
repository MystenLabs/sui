// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A

//# publish
module test::m {
    use sui::dynamic_object_field as ofield;

    public struct Parent has key, store {
        id: UID,
    }

    public struct Child has key, store {
        id: UID,
        value: u64,
    }

    public struct OtherChild has key, store {
        id: UID,
        value: u64,
    }

    public entry fun parent(ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx) },
            tx_context::sender(ctx),
        )
    }

    public entry fun add_child(parent: &mut Parent, ctx: &mut TxContext) {
        let child = Child { id: object::new(ctx), value: 10 };
        let child_id = object::id(&child);

        ofield::add(&mut parent.id, 0u64, child);
        let stored_id = ofield::id(&parent.id, 0u64);

        assert!(stored_id.is_some(), 0);
        assert!(stored_id.destroy_some() == child_id, 1);
        assert!(ofield::exists_with_type<u64, Child>(&parent.id, 0u64), 2);
        assert!(!ofield::exists_with_type<u64, OtherChild>(&parent.id, 0u64), 3);
    }

    public entry fun replace_child(parent: &mut Parent, ctx: &mut TxContext) {
        let replacement = OtherChild { id: object::new(ctx), value: 20 };
        let replacement_id = object::id(&replacement);
        let old = ofield::replace<u64, OtherChild, Child>(&mut parent.id, 0u64, replacement);

        assert!(old.is_some(), 4);

        let Child { id, value } = old.destroy_some();
        assert!(value == 10, 5);
        object::delete(id);

        let stored_id = ofield::id(&parent.id, 0u64);
        assert!(stored_id.is_some(), 6);
        assert!(stored_id.destroy_some() == replacement_id, 7);
        assert!(!ofield::exists_with_type<u64, Child>(&parent.id, 0u64), 8);
        assert!(ofield::exists_with_type<u64, OtherChild>(&parent.id, 0u64), 9);
    }

    public entry fun remove_child(parent: &mut Parent) {
        let OtherChild { id, value } = ofield::remove<u64, OtherChild>(&mut parent.id, 0u64);

        assert!(value == 20, 10);
        object::delete(id);

        let stored_id = ofield::id(&parent.id, 0u64);
        assert!(stored_id.is_none(), 11);
        assert!(!ofield::exists<u64>(&parent.id, 0u64), 12);
    }
}

//# run test::m::parent --sender A

//# run test::m::add_child --sender A --args object(2,0)

//# run test::m::replace_child --sender A --args object(2,0)

//# run test::m::remove_child --sender A --args object(2,0)
