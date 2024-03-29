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

    public entry fun deleter(parent: &mut A, x: Receiving<B>) {
        let B { id } = transfer::receive(&mut parent.id, x);
        object::delete(id);
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Receive and delete the received object
//# run tto::M1::deleter --args object(2,0) receiving(2,1)

//# view-object 2,0

//# view-object 2,1

// Try and receive the same object again -- should fail
//# run tto::M1::deleter --args object(2,0) receiving(2,1)@3
