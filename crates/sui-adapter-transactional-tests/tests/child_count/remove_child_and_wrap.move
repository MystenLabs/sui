// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests that a parent object can be deleted, after it was wrapped in the same txn where it lost its
// last parent

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

    public entry fun create(ctx: &mut TxContext) {
        let id = sui::object::new(ctx);
        let child = S { id: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut id);
        let parent = S { id };
        sui::transfer::transfer(parent, tx_context::sender(ctx))
    }

    public entry fun delete_and_wrap(parent: S, child: S, ctx: &mut TxContext) {
        let S { id } = child;
        sui::object::delete(id);
        let r = R { id: sui::object::new(ctx), s: parent };
        sui::transfer::transfer(r, tx_context::sender(ctx))
    }

    public entry fun unwrap_and_delete(r: R) {
        let R { id, s } = r;
        sui::object::delete(id);
        let S { id } = s;
        sui::object::delete(id);
    }
}

//
// Test wrapping allows non-zero child count
//

//# run test::m::create --sender A

//# view-object 107

//# run test::m::delete_and_wrap --sender A --args object(107) object(108)

//# view-object 110

//# run test::m::unwrap_and_delete --sender A --args object(110)

//# view-object 107

//# view-object 110
