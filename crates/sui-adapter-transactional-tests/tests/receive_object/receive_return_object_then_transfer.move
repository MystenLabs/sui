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

    public fun receiver(parent: &mut A, x: Receiving<B>): B {
        transfer::receive(&mut parent.id, x)
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Can receive the object, but if you don't do anything with it the transaction will fail
//# programmable --inputs object(2,0) receiving(2,1) @tto
//> 0: tto::M1::receiver(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# view-object 2,0

//# view-object 2,1
