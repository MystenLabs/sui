// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that repacking does not let an existing owned or wrapped object become shared, since the
// runtime only shares objects created in the current transaction.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID }
    public struct B has key { id: UID }
    public struct Holder has key { id: UID, inner: UID }

    public fun mint(ctx: &mut TxContext) {
        transfer::transfer(A { id: object::new(ctx) }, tx_context::sender(ctx))
    }

    public fun a_to_b_share(a: A) {
        let A { id } = a;
        transfer::share_object(B { id })
    }

    public fun wrap(a: A, ctx: &mut TxContext) {
        let A { id } = a;
        transfer::transfer(Holder { id: object::new(ctx), inner: id }, tx_context::sender(ctx))
    }

    public fun unwrap_share(h: Holder) {
        let Holder { id, inner } = h;
        object::delete(id);
        transfer::share_object(B { id: inner })
    }
}

//# run ex::m::mint --sender A

//# run ex::m::a_to_b_share --args object(2,0) --sender A

//# view-object 2,0

//# run ex::m::wrap --args object(2,0) --sender A

//# run ex::m::unwrap_share --args object(5,0) --sender A

//# view-object 5,0
