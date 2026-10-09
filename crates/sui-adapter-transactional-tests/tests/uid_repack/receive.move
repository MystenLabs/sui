// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests repacking an object received from a parent object into a different type.

//# init --addresses ex=0x0 --accounts A --flavor core

//# publish
module ex::m {
    use sui::object::{Self, UID};
    use sui::transfer::{Self, Receiving};
    use sui::tx_context::{Self, TxContext};

    public struct Parent has key { id: UID }
    public struct A has key { id: UID }
    public struct B has key { id: UID }

    public fun start(ctx: &mut TxContext) {
        let p = Parent { id: object::new(ctx) };
        let p_address = object::uid_to_address(&p.id);
        transfer::transfer(A { id: object::new(ctx) }, p_address);
        transfer::transfer(p, tx_context::sender(ctx))
    }

    // Sends the repacked object back to the parent
    public fun receive_a_as_b(p: &mut Parent, r: Receiving<A>) {
        let A { id } = transfer::receive(&mut p.id, r);
        transfer::transfer(B { id }, object::uid_to_address(&p.id))
    }

    public fun receive_b_as_a(p: &mut Parent, r: Receiving<B>, ctx: &TxContext) {
        let B { id } = transfer::receive(&mut p.id, r);
        transfer::transfer(A { id }, tx_context::sender(ctx))
    }

    public fun receive_a(p: &mut Parent, r: Receiving<A>, ctx: &TxContext) {
        let a = transfer::receive(&mut p.id, r);
        transfer::transfer(a, tx_context::sender(ctx))
    }
}

//# run ex::m::start --sender A

//# view-object 2,1

//# run ex::m::receive_a_as_b --args object(2,0) receiving(2,1) --sender A

//# view-object 2,1

// The object is no longer an A
//# run ex::m::receive_a --args object(2,0) receiving(2,1) --sender A

//# run ex::m::receive_b_as_a --args object(2,0) receiving(2,1) --sender A

//# view-object 2,1
