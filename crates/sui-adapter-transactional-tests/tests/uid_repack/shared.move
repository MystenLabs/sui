// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests repacking the UID of a shared object taken by value. The repacked object must still be
// re-shared or deleted.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID }
    public struct B has key { id: UID }
    public struct Holder has key { id: UID, inner: UID }

    public fun create(ctx: &mut TxContext) {
        transfer::share_object(A { id: object::new(ctx) })
    }

    public fun repack_a_share(a: A) {
        let A { id } = a;
        transfer::share_object(A { id })
    }

    public fun a_to_b_share(a: A) {
        let A { id } = a;
        transfer::share_object(B { id })
    }

    public fun b_to_a_share(b: B) {
        let B { id } = b;
        transfer::share_object(A { id })
    }

    public fun b_to_a_transfer(b: B, ctx: &TxContext) {
        let B { id } = b;
        transfer::transfer(A { id }, tx_context::sender(ctx))
    }

    public fun b_to_a_freeze(b: B) {
        let B { id } = b;
        transfer::freeze_object(A { id })
    }

    public fun b_wrap_uid(b: B, ctx: &mut TxContext) {
        let B { id } = b;
        transfer::transfer(Holder { id: object::new(ctx), inner: id }, tx_context::sender(ctx))
    }

    public fun a_to_b_delete(a: A) {
        let A { id } = a;
        let B { id } = B { id };
        object::delete(id)
    }

    public fun use_a(_: &A) {}

    public fun use_b(_: &B) {}
}

//# run ex::m::create

//# view-object 2,0

//# run ex::m::repack_a_share --args object(2,0)

//# view-object 2,0

//# run ex::m::a_to_b_share --args object(2,0)

//# view-object 2,0

//# run ex::m::use_b --args object(2,0)

// A shared object cannot be transferred, frozen, or wrapped after a repack
//# run ex::m::b_to_a_transfer --args object(2,0)

//# run ex::m::b_to_a_freeze --args object(2,0)

//# run ex::m::b_wrap_uid --args object(2,0)

//# view-object 2,0

//# run ex::m::b_to_a_share --args object(2,0)

//# view-object 2,0

//# run ex::m::use_a --args object(2,0)

//# run ex::m::a_to_b_delete --args object(2,0)

//# view-object 2,0
