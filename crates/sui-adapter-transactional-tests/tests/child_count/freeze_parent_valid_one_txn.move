// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests valid freezing of an object that has children
// child is deleted and parent is frozen in one transaction

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::tx_context::{Self, TxContext};

    struct S has key, store {
        id: sui::object::UID,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let s = S { id: sui::object::new(ctx) };
        sui::transfer::transfer(s, tx_context::sender(ctx))
    }

    public entry fun transfer(s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

    public entry fun transfer_to_object(child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

    public entry fun freeze_and_delete(parent: S, child: S) {
        sui::transfer::freeze_object(parent);
        let S { id } = child;
        sui::object::delete(id);
    }

    public entry fun delete_and_freeze(child: S, parent: S) {
        let S { id } = child;
        sui::object::delete(id);
        sui::transfer::freeze_object(parent);
    }
}

//
// Test freezing parent then deleting child, in the same txn
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(109) object(107)

//# view-object 107

//# run test::m::freeze_and_delete --sender A --args object(107) object(109)

//
// Test deleting child then freezing parent, in the same txn
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(115) object(113)

//# view-object 113

//# run test::m::delete_and_freeze --sender A --args object(115) object(113)
