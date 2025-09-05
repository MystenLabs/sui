// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that reach receiving usage by type has a different logical value/local in PTB execution

//# init --addresses tto=0x0

//# publish
module tto::m1 {
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

    public fun start(ctx: &mut TxContext) {
        let a = A { id: object::new(ctx) };
        let a_address = object::id_address(&a);
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(B { id: object::new(ctx) }, a_address);
        transfer::public_transfer(B { id: object::new(ctx) }, a_address);
    }

    public fun id<T: key>(x: Receiving<T>): Receiving<T> {
        x
    }

    public fun borrow_mut<T: key>(x: &mut Receiving<T>): &mut Receiving<T> {
        x
    }

    public fun receive(parent: &mut A, x: Receiving<B>) {
        let b = transfer::receive(&mut parent.id, x);
        transfer::public_transfer(b, @tto);
    }

    public fun take_all(parent: &mut A, a: Receiving<A>, b: Receiving<B>, c: Receiving<C>) {
        assert!(transfer::receiving_object_id(&a) == transfer::receiving_object_id(&b));
        assert!(transfer::receiving_object_id(&b) == transfer::receiving_object_id(&c));
        receive(parent, b);
    }

    public fun take_two_b(parent: &mut A, b1: Receiving<B>, b2: Receiving<B>, ) {
        receive(parent, b1);
        receive(parent, b2);
    }

}

//# run tto::m1::start

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

// Can use receiving multiple times with different types
//# programmable --inputs receiving(2,1) --dev-inspect
//> 0: tto::m1::borrow_mut<tto::m1::A>(Input(0));
//> tto::m1::id<tto::m1::B>(Input(0));
//> tto::m1::id<tto::m1::C>(Input(0));
//> tto::m1::borrow_mut<tto::m1::A>(Result(0));

// But we cannot use them multiple times (by value) with the same type
//# programmable --inputs receiving(2,1)
//> tto::m1::id<tto::m1::A>(Input(0));
//> tto::m1::id<tto::m1::B>(Input(0));
//> tto::m1::id<tto::m1::C>(Input(0));
//> tto::m1::id<tto::m1::A>(Input(0));

// And can receive one of them
//# programmable --inputs object(2,0) receiving(2,1)
//> tto::m1::id<tto::m1::A>(Input(1));
//> 1: tto::m1::id<tto::m1::B>(Input(1));
//> tto::m1::id<tto::m1::C>(Input(1));
//> tto::m1::receive(Input(0), Result(1));

// Cannot double take the same receiving input twice at the same type
//# programmable --inputs object(2,0) receiving(2,2)
//> tto::m1::take_two_b(Input(0), Input(1), Input(1))

// But can use receiving multiple times with different types all at once
//# programmable --inputs object(2,0) receiving(2,2)
//> tto::m1::take_all(Input(0), Input(1), Input(1), Input(1))
