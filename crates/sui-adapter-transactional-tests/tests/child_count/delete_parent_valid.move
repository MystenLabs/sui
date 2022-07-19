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

    public entry fun mint(ctx: &mut TxContext) {
        let s = S { info: sui::object::new(ctx) };
        sui::transfer::transfer(s, tx_context::sender(ctx))
    }

    public entry fun transfer(s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

    public entry fun transfer_to_object(child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

    public entry fun delete_child(_parent: &S, child: S) {
        let S { info } = child;
        sui::object::delete(info)
    }

    public entry fun delete(s: S) {
        let S { info } = s;
        sui::object::delete(info)
    }

}

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(109) object(107)

//# view-object 107

//# run test::m::delete_child --sender A --args object(107) object(109)

//# run test::m::delete --sender A --args object(107)
