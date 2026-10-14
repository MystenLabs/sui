// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests repacking the UID of an owned object into the same type and into a different type.
// Uses the core flavor since the Sui compiler still rejects packing an existing UID.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    public struct A has key { id: UID, value: u64 }
    public struct B has key, store { id: UID, value: u64 }

    public fun mint(ctx: &mut TxContext) {
        transfer::transfer(A { id: object::new(ctx), value: 0 }, tx_context::sender(ctx))
    }

    public fun repack_a(a: A, ctx: &TxContext) {
        let A { id, value } = a;
        transfer::transfer(A { id, value: value + 1 }, tx_context::sender(ctx))
    }

    public fun a_to_b(a: A): B {
        let A { id, value } = a;
        B { id, value: value + 1 }
    }

    public fun b_to_a(b: B, ctx: &TxContext) {
        let B { id, value } = b;
        transfer::transfer(A { id, value: value + 1 }, tx_context::sender(ctx))
    }

    public fun use_a(_: &A) {}

    public fun use_b(_: &B) {}
}

//# run ex::m::mint --sender A

//# view-object 2,0

// Same-type repack is a plain mutation
//# run ex::m::repack_a --args object(2,0) --sender A

//# view-object 2,0

// B has store, so the PTB can transfer the repacked object
//# programmable --inputs object(2,0) @A --sender A
//> 0: ex::m::a_to_b(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# view-object 2,0

// The object is now a B
//# run ex::m::use_a --args object(2,0) --sender A

//# run ex::m::use_b --args object(2,0) --sender A

//# run ex::m::b_to_a --args object(2,0) --sender A

//# view-object 2,0

//# run ex::m::use_a --args object(2,0) --sender A
