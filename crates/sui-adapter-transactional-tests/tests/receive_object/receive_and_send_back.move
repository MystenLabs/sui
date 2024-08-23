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

    public entry fun send_back(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        let parent_address = object::id_address(parent);
        transfer::public_transfer(b, parent_address);
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Receive the object and send it back to the object we received it from.
//# run tto::M1::send_back --args object(2,0) receiving(2,1)

//# view-object 2,0

//# view-object 2,1

// Make sure that we cannot receive it again at the old version number
//# run tto::M1::send_back --args object(2,0) receiving(2,1)@3
