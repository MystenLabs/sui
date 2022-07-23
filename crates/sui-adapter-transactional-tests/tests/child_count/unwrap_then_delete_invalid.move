// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid deletion of an object that has children

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

    public entry fun transfer_to_object(child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

    public entry fun wrap(s: S, ctx: &mut TxContext) {
        let r = R { info: sui::object::new(ctx), s };
        sui::transfer::transfer(r, tx_context::sender(ctx))
    }

    public entry fun delete(r: R) {
        let R { info, s } = r;
        sui::object::delete(info);
        let S { info } = s;
        sui::object::delete(info);
    }
}

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(109) object(107)

//# run test::m::wrap --sender A --args object(107)

//# view-object 112

//# run test::m::delete --sender A --args object(112)
