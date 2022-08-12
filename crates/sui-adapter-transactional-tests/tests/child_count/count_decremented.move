// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various ways of "removing" a child decrements the count

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::tx_context::{Self, TxContext};

    struct S has key, store {
        id: sui::object::UID,
    }

    struct R has key {
        id: sui::object::UID,
        s: S,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let parent = sui::object::new(ctx);
        let child = S { id: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut parent);
        sui::transfer::transfer(S { id: parent }, tx_context::sender(ctx))
    }

    public entry fun share_object(_parent: &S, s: S) {
        sui::transfer::share_object(s)
    }

    public entry fun freeze_object(_parent: &S, s: S) {
        sui::transfer::freeze_object(s)
    }

    public entry fun transfer_child(_parent: &S, s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

    public entry fun transfer_to_object(_old_parent: &S, child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

    public entry fun transfer_to_object_id(_old_parent: &S, child: S, parent: &mut S) {
        sui::transfer::transfer_to_object_id(child, &mut parent.id)
    }

    public entry fun delete(_old_parent: &S, child: S) {
        let S { id } = child;
        sui::object::delete(id);
    }

    public entry fun wrap(_parent: &S, s: S, ctx: &mut TxContext) {
        let r = R { id: sui::object::new(ctx), s };
        sui::transfer::transfer(r, tx_context::sender(ctx))
    }
}

//
// Test sharing
//

//# run test::m::mint --sender A

//# view-object 107

//# run test::m::share_object --sender A --args object(107) object(108)

//# view-object 107

//
// Test freezing
//

//# run test::m::mint --sender A

//# view-object 112

//# run test::m::freeze_object --sender A --args object(112) object(111)

//# view-object 112

//
// Test transfer
//

//# run test::m::mint --sender A

//# view-object 115

//# run test::m::transfer_child --sender A --args object(115) object(116) @B

//# view-object 115

//
// Test transfer to object
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# view-object 120

//# run test::m::transfer_to_object --sender A --args object(120) object(119) object(123)

//# view-object 120

//
// Test transfer to id
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# view-object 127

//# run test::m::transfer_to_object_id --sender A --args object(127) object(126) object(130)

//# view-object 127

//
// Test delete
//

//# run test::m::mint --sender A

//# view-object 134

//# run test::m::delete --sender A --args object(134) object(133)

//# view-object 134

//
// Test wrap
//

//# run test::m::mint --sender A

//# view-object 137

//# run test::m::wrap --sender A --args object(137) object(138)

//# view-object 137
