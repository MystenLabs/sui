// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests deleting a wrapped object that has never been in storage

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    public struct S has key, store {
        id: sui::object::UID,
    }

    public struct R has key {
        id: sui::object::UID,
        s: S,
    }

    public entry fun create(ctx: &mut TxContext) {
        let parent = sui::object::new(ctx);
        let child = S { id: sui::object::new(ctx) };
        sui::transfer::transfer(R { id: parent, s: child }, tx_context::sender(ctx))
    }

    public entry fun delete(r: R) {
        let R { id, s } = r;
        sui::object::delete(id);
        let S { id } = s;
        sui::object::delete(id);
    }
}

//
// Test sharing
//

//# run test::m::create --sender A

//# run test::m::delete --args object(2,0) --sender A
