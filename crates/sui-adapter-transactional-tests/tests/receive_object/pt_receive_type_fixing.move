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

    public entry fun pass_through(x: Receiving<B>): Receiving<B> { x }

    public entry fun pass_through_a(x: Receiving<A>): Receiving<A> { x }

    public entry fun pass_through_ref_a(_x: &Receiving<A>) { }

    public entry fun pass_through_mut_ref_a(_x: &mut Receiving<A>) { }

    public fun unpacker_a(mut x: vector<Receiving<A>>): Receiving<A> {
        vector::pop_back(&mut x)
    }

    public fun unpacker_b(mut x: vector<Receiving<B>>): Receiving<B> {
        vector::pop_back(&mut x)
    }

    public fun unpacker_generic<T: key + store>(mut x: vector<Receiving<T>>): Receiving<T> {
        vector::pop_back(&mut x)
    }

    public entry fun receiver(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        transfer::public_transfer(b, @tto);
    }
}

//# run tto::M1::start

//# view-object 2,0

//# view-object 2,1

// Pass a receiving object through a move function, and then use the returned
// Receiving argument in a subsequent command.
//# programmable --inputs object(2,0) receiving(2,1)
//> 0: tto::M1::pass_through(Input(1));
//> tto::M1::receiver(Input(0), Result(0));

//# programmable --inputs object(2,0) receiving(2,1)
//> 0: tto::M1::pass_through_a(Input(1));
//> tto::M1::receiver(Input(0), Result(0));

//# programmable --inputs object(2,0) receiving(2,1)
//> 0: tto::M1::pass_through_mut_ref_a(Input(1));
//> tto::M1::receiver(Input(0), Input(1));

//# programmable --inputs object(2,0) receiving(2,1)
//> 0: tto::M1::pass_through_ref_a(Input(1));
//> tto::M1::receiver(Input(0), Input(1));

// make vec, then unpack it and make sure the type is fixed
//# programmable --inputs object(2,0) receiving(2,1)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::A>>([Input(1)]);
//> 1: tto::M1::unpacker_b(Result(0));

//# programmable --inputs object(2,0) receiving(2,1)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::A>>([Input(1)]);
//> 1: tto::M1::unpacker_a(Result(0));
//> 2: tto::M1::receiver(Input(0), Result(1));

// This is fine since we are going A -> A in the unpack. But we should fail the call.
//# programmable --inputs object(2,0) receiving(2,1)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::A>>([Input(1)]);
//> 1: tto::M1::unpacker_generic<tto::M1::A>(Result(0));
//> 2: tto::M1::receiver(Input(0), Result(1));

// This should fail since we're going A -> B in the unpack.
//# programmable --inputs object(2,0) receiving(2,1)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::A>>([Input(1)]);
//> 1: tto::M1::unpacker_generic<tto::M1::B>(Result(0));
