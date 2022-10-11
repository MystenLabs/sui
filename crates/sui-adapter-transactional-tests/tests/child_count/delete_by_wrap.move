// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// DEPRECATED child count no longer tracked
// tests invalid wrapping of a parent object with children

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
        let s = S { id: sui::object::new(ctx) };
        sui::transfer::transfer(s, tx_context::sender(ctx))
    }

    public entry fun share(s: S) {
        sui::transfer::share_object(s)
    }

    public entry fun transfer(s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

    public entry fun transfer_to_object(child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

    public entry fun wrap(s: S, ctx: &mut TxContext) {
        let r = R { id: sui::object::new(ctx), s };
        sui::transfer::transfer(r, tx_context::sender(ctx))
    }
}

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(107) object(109)

//# view-object 109

//# run test::m::wrap --sender A --args object(109)
