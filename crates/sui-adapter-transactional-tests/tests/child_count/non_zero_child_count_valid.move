// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests valid transfers of an object that has children

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::tx_context::{Self, TxContext};
    use sui::dynamic_object_field as ofield;

    struct S has key, store {
        id: sui::object::UID,
    }

    struct R has key, store {
        id: sui::object::UID,
        s: S,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let id = sui::object::new(ctx);
        let child = S { id: sui::object::new(ctx) };
        ofield::add(&mut id, 0, child);
        sui::transfer::transfer(S { id }, tx_context::sender(ctx))
    }

    public entry fun mint_and_share(ctx: &mut TxContext) {
        let id = sui::object::new(ctx);
        let child = S { id: sui::object::new(ctx) };
        ofield::add(&mut id, 0, child);
        sui::transfer::share_object(S { id })
    }

    public entry fun transfer(s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

}

//
// Test share object allows non-zero child count
//

//# run test::m::mint_and_share --sender A

//# view-object 108

//
// Test transfer allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::transfer --sender A --args object(112) @B

//# view-object 112

//
// Test TransferObject allows non-zero child count
//

//# run test::m::mint --sender A

//# transfer-object 117 --sender A --recipient B

//# view-object 117
