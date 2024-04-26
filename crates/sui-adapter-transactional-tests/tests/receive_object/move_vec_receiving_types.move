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

    public struct C has key, store {
        id: UID,
    }

    public struct RecvSpoof<phantom T: key> has drop {
        id: ID,
        version: u64,
    }

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        let b = B { id: object::new(ctx) };
        let c = B { id: object::new(ctx) };
        let d = C { id: object::new(ctx) };
        let e = C { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
        transfer::public_transfer(c, a_address);
        transfer::public_transfer(d, a_address);
        transfer::public_transfer(e, a_address);
    }

    public fun make_recv_spoof_b(): RecvSpoof<B> {
        RecvSpoof {
            id: object::id_from_address(@0x0),
            version: 0,
        }
    }

    public fun spoof_bytes(r: RecvSpoof<B>): vector<u8> {
        std::bcs::to_bytes(&r)
    }

    public fun receive_none(_v: vector<Receiving<B>>){ }

    public fun receive_none_a(_v: vector<Receiving<A>>){ }

    public fun receive_all(parent: &mut A, mut x: vector<Receiving<B>>) {
        while (!vector::is_empty(&x)) {
            let r = vector::pop_back(&mut x);
            let b = transfer::receive(&mut parent.id, r);
            transfer::public_transfer(b, @tto);
        };
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3

//# view-object 2,4

// Receiving arguments are untyped at the PTB level
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);

// As long as you don't load the object the type will not be checked.
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_none(Result(0));

// Try to pass the wrong-type move vec to the function
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_none_a(Result(0));

// If you try to receive an object at the wrong type, it will fail
// E_RECEIVING_TYPE_MISMATCH
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all(Input(0), Result(0));

// Try to spoof a receiving object
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: tto::M1::make_recv_spoof_b();
//> 1: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4), Result(0)]);

//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: tto::M1::make_recv_spoof_b();
//> 1: tto::M1::spoof_bytes(Result(0));
//> 2: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4), Result(1)]);
