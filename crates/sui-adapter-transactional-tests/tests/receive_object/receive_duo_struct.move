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

    public struct Duo<phantom T: key> has drop {
        r1: Receiving<T>,
        r2: Receiving<T>,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        let b_address = object::id_address(&b);
        let c = B { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
        transfer::public_transfer(c, b_address);
    }

    public fun make_duo(r1: Receiving<B>, r2: Receiving<B>): Duo<B> {
        Duo { r1, r2 }
    }

    public fun receive_duo(parent: &mut A, d: Duo<B>): (B, B) {
        let Duo { r1, r2 } = d;
        let mut r1 = transfer::receive(&mut parent.id, r1);
        let r2 = transfer::receive(&mut r1.id, r2);
        (r1, r2)
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

// Can drop duo
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2)
//> 0: tto::M1::make_duo(Input(1), Input(2))

// receive the objects and return them. Error since we need to do something with the returned objects
//# programmable --inputs object(2,0) receiving(2,2) receiving(2,2)
//> 0: tto::M1::make_duo(Input(1), Input(2));
//> 1: tto::M1::receive_duo(Input(0), Result(0));

// receive the objects and return them. Then Transfer them with TransferObjects
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) @tto
//> 0: tto::M1::make_duo(Input(1), Input(2));
//> 1: tto::M1::receive_duo(Input(0), Result(0));
//> 2: TransferObjects([NestedResult(1, 0), NestedResult(1, 1)], Input(3));
