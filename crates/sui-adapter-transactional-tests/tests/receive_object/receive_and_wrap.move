// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses tto=0x0

//# publish
module tto::M1 {
    use sui::transfer::Receiving;

    public struct Wrapper has key, store {
        id: UID,
        elem: B
    }

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

    public entry fun wrapper(parent: &mut A, x: Receiving<B>, ctx: &mut TxContext) {
        let b = transfer::receive(&mut parent.id, x);
        let wrapper = Wrapper {
            id: object::new(ctx),
            elem: b
        };
        transfer::public_transfer(wrapper, @tto);
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Receive wrap and then transfer the wrapped object
//# run tto::M1::wrapper --args object(2,0) receiving(2,1)

//# view-object 2,0

//# view-object 2,1

// Try an receive at the old version -- should fail
//# run tto::M1::wrapper --args object(2,0) receiving(2,1)@3
