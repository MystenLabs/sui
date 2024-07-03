// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests that the parent cannot be deleted while it has children

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::dynamic_object_field as ofield;

    public struct S has key, store {
        id: sui::object::UID,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let id = sui::object::new(ctx);
        sui::transfer::public_transfer(S { id }, tx_context::sender(ctx))
    }

    public entry fun add(parent: &mut S, idx: u64, ctx: &mut TxContext) {
        let child = S { id: sui::object::new(ctx) };
        ofield::add(&mut parent.id, idx, child);
    }

    public entry fun delete(s: S) {
        let S { id } = s;
        sui::object::delete(id)
    }
}

//# run test::m::mint --sender A

//# run test::m::add --sender A --args object(2,0) 0

//# view-object 2,0

//# run test::m::delete --sender A --args object(2,0)
