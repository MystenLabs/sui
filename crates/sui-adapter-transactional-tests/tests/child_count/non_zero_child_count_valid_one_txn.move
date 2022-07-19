// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid transfers of an object that has children
// all transfers done in a single transaction

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::tx_context::{Self, TxContext};

    struct S has key, store {
        info: sui::object::Info,
    }

    struct R has key {
        info: sui::object::Info,
        s: S,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let s = S { info: sui::object::new(ctx) };
        sui::transfer::transfer(s, tx_context::sender(ctx))
    }

    public entry fun test_transfer_to_object(super_parent: &mut S, ctx: &mut TxContext) {
        let info = sui::object::new(ctx);
        let child = S { info: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut info);
        let parent = S { info };
        sui::transfer::transfer_to_object(parent, super_parent)
    }

    public entry fun test_transfer(recipient: address, ctx: &mut TxContext) {
        let info = sui::object::new(ctx);
        let child = S { info: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut info);
        let parent = S { info };
        sui::transfer::transfer(parent, recipient)
    }

    public entry fun test_share(ctx: &mut TxContext) {
        let info = sui::object::new(ctx);
        let child = S { info: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut info);
        let parent = S { info };
        sui::transfer::share_object(parent)
    }

    public entry fun test_wrap(ctx: &mut TxContext) {
        let info = sui::object::new(ctx);
        let child = S { info: sui::object::new(ctx) };
        sui::transfer::transfer_to_object_id(child, &mut info);
        let parent = S { info };
        let r = R { info: sui::object::new(ctx), s: parent };
        sui::transfer::transfer(r, tx_context::sender(ctx))
    }
}

//
// Test transfer_to_object allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::test_transfer_to_object --sender A --args object(107)


//
// Test share object allows non-zero child count
//

//# run test::m::test_share --sender A

//
// Test transfer allows non-zero child count
//

//# run test::m::test_transfer --sender A --args @B

//
// Test wrapping allows non-zero child count
//

//# run test::m::test_wrap --sender A
