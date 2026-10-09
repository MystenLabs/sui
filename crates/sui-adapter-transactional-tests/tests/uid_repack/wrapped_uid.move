// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests storing the UID of an object (wrapping it) and later packing it into an object again,
// possibly with a different type. The object reappears as unwrapped.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::dynamic_field;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID, value: u64 }
    public struct B has key { id: UID }
    public struct Holder has key { id: UID, inner: UID }
    public struct Parent has key { id: UID }

    public fun mint(ctx: &mut TxContext) {
        transfer::transfer(A { id: object::new(ctx), value: 0 }, tx_context::sender(ctx));
        transfer::transfer(Parent { id: object::new(ctx) }, tx_context::sender(ctx))
    }

    public fun wrap(a: A, ctx: &mut TxContext) {
        let A { id, value: _ } = a;
        transfer::transfer(Holder { id: object::new(ctx), inner: id }, tx_context::sender(ctx))
    }

    public fun unwrap_as_b(h: Holder, ctx: &TxContext) {
        let Holder { id, inner } = h;
        object::delete(id);
        transfer::transfer(B { id: inner }, tx_context::sender(ctx))
    }

    public fun stash(p: &mut Parent, b: B) {
        let B { id } = b;
        dynamic_field::add(&mut p.id, 0u64, id)
    }

    public fun unstash_as_a(p: &mut Parent, ctx: &TxContext) {
        let id: UID = dynamic_field::remove(&mut p.id, 0u64);
        transfer::transfer(A { id, value: 7 }, tx_context::sender(ctx))
    }

    public fun wrap_then_delete(a: A, ctx: &mut TxContext) {
        let A { id, value: _ } = a;
        let h = Holder { id: object::new(ctx), inner: id };
        let Holder { id, inner } = h;
        object::delete(id);
        transfer::transfer(B { id: inner }, tx_context::sender(ctx))
    }

    public fun delete_inner(h: Holder) {
        let Holder { id, inner } = h;
        object::delete(id);
        object::delete(inner)
    }
}

//# run ex::m::mint --sender A

//# view-object 2,0

// A is wrapped as a bare UID
//# run ex::m::wrap --args object(2,0) --sender A

// A is unwrapped as a B
//# run ex::m::unwrap_as_b --args object(4,0) --sender A

//# view-object 2,0

// B is wrapped as a bare UID in a dynamic field
//# run ex::m::stash --args object(2,1) object(2,0) --sender A

// B is unwrapped as an A
//# run ex::m::unstash_as_a --args object(2,1) --sender A

//# view-object 2,0

// Wrapping and unwrapping within a transaction is a plain mutation
//# run ex::m::wrap_then_delete --args object(2,0) --sender A

//# view-object 2,0

//# run ex::m::mint --sender A

//# run ex::m::wrap --args object(12,0) --sender A

// Deleting a bare wrapped UID
//# run ex::m::delete_inner --args object(13,0) --sender A
