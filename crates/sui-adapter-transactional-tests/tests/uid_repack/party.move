// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests repacking the UID of a party object into a different type.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::party;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID }
    public struct B has key { id: UID }

    public fun mint(ctx: &mut TxContext) {
        transfer::party_transfer(
            A { id: object::new(ctx) },
            party::single_owner(tx_context::sender(ctx)),
        )
    }

    public fun a_to_b_party(a: A, ctx: &TxContext) {
        let A { id } = a;
        transfer::party_transfer(B { id }, party::single_owner(tx_context::sender(ctx)))
    }

    public fun b_to_a_transfer(b: B, ctx: &TxContext) {
        let B { id } = b;
        transfer::transfer(A { id }, tx_context::sender(ctx))
    }
}

//# run ex::m::mint --sender A

//# view-object 2,0

//# run ex::m::a_to_b_party --args object(2,0) --sender A

//# view-object 2,0

//# run ex::m::b_to_a_transfer --args object(2,0) --sender A

//# view-object 2,0
