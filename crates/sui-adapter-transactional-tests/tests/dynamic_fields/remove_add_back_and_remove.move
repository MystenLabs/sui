// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test attempts to remove a child, add it back, remove it again, and then transfer/delete it.
// This is an interesting test case because when child objects are removed, added back and removed again,
// they won't show up in the child_object_effects in the object runtiem. We must look at either transfers
// or deleted_object_ids to figure them out.

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Object has key, store {
        id: UID,
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Object { id: object::new(ctx) };
        transfer::public_transfer(o, tx_context::sender(ctx))
    }

    public entry fun add_child(parent: &mut Object, ctx: &mut TxContext) {
        let child = Object { id: object::new(ctx) };
        sui::dynamic_object_field::add(&mut parent.id, 0, child);
    }

    public fun transfer_child(parent: &mut Object, ctx: &TxContext) {
        let child: Object = sui::dynamic_object_field::remove(&mut parent.id, 0);
        sui::dynamic_object_field::add(&mut parent.id, 1, child);
        let child: Object = sui::dynamic_object_field::remove(&mut parent.id, 1);
        transfer::public_transfer(child, tx_context::sender(ctx))
    }

    public fun delete_child(parent: &mut Object) {
        let child: Object = sui::dynamic_object_field::remove(&mut parent.id, 0);
        sui::dynamic_object_field::add(&mut parent.id, 1, child);
        let child: Object = sui::dynamic_object_field::remove(&mut parent.id, 1);
        let Object { id } = child;
        object::delete(id);
    }
}

//# run test::m1::create --sender A

//# run test::m1::add_child --args object(2,0) --sender A

//# run test::m1::transfer_child --args object(2,0) --sender A

//# run test::m1::add_child --args object(2,0) --sender A

//# run test::m1::delete_child --args object(2,0) --sender A
