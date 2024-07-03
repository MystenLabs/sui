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
        transfer::public_share_object(a);
        transfer::public_transfer(b, a_address);
    }

    public entry fun send_back(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        let parent_address = object::id_address(parent);
        transfer::public_transfer(b, parent_address);
    }

    public entry fun nop(_parent: &mut A) { }
    public entry fun nop_with_receiver(_parent: &mut A, _x: Receiving<B>) { }
}

//# run tto::M1::start

//# programmable --inputs object(2,0) receiving(2,1)
//> tto::M1::send_back(Input(0), Input(1))

// Include the receiving argument, but don't use it at the PTB level
//# programmable --inputs object(2,0) receiving(2,1)
//> tto::M1::nop(Input(0))

// Include the receiving argument, but don't use it at the Move level. The
// receiving object should not be mutated by this.
//# programmable --inputs object(2,0) receiving(2,1)
//> tto::M1::nop_with_receiver(Input(0), Input(1))
