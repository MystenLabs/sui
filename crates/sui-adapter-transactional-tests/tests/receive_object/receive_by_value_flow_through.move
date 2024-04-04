// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0

//# publish
module tto::M1 {
    use sui::transfer::Receiving;

    public struct A has key, store {
        id: UID,
    }

    public struct B has key, store {
        id: UID,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
    }

    public fun flow(_parent: &mut A, x: Receiving<B>): Receiving<B> { x }
    public fun drop(_parent: &mut A, _x: Receiving<B>) { }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Flow through but don't use the receiving argument. 2,0 should be updated but 2,1 should not.
//# run tto::M1::flow --args object(2,0) receiving(2,1)

// Drop the receiving argument. 2,0 should be updated but 2,1 should not.
//# run tto::M1::drop --args object(2,0) receiving(2,1)
