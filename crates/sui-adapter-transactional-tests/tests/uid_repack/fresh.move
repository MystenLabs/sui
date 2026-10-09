// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests repacking a UID created in the same transaction. The object is reported as created with
// its final type, and creating then deleting it leaves no trace in effects.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID }
    public struct B has key { id: UID }

    fun new_b(ctx: &mut TxContext): B {
        let a = A { id: object::new(ctx) };
        let A { id } = a;
        B { id }
    }

    public fun transfer_b(ctx: &mut TxContext) {
        transfer::transfer(new_b(ctx), tx_context::sender(ctx))
    }

    public fun share_b(ctx: &mut TxContext) {
        transfer::share_object(new_b(ctx))
    }

    public fun freeze_b(ctx: &mut TxContext) {
        transfer::freeze_object(new_b(ctx))
    }

    public fun delete_b(ctx: &mut TxContext) {
        let B { id } = new_b(ctx);
        object::delete(id)
    }

    public fun use_b(_: &B) {}
}

//# run ex::m::transfer_b --sender A

//# view-object 2,0

//# run ex::m::share_b --sender A

//# view-object 4,0

//# run ex::m::freeze_b --sender A

//# view-object 6,0

//# run ex::m::use_b --args object(4,0) --sender A

//# run ex::m::delete_b --sender A
