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
        let c = B { id: object::new(ctx) };
        let d = B { id: object::new(ctx) };
        let e = B { id: object::new(ctx) };
        transfer::public_transfer(a, tx_context::sender(ctx));
        transfer::public_transfer(b, a_address);
        transfer::public_transfer(c, a_address);
        transfer::public_transfer(d, a_address);
        transfer::public_transfer(e, a_address);
    }

    public fun receive_none(_parent: &mut A, _x: vector<Receiving<B>>) { }

    public fun receive_none_by_immref(_parent: &mut A, _x: &vector<Receiving<B>>) { }

    public fun receive_none_by_mutref(_parent: &mut A, _x: &vector<Receiving<B>>) { }

    public fun receive_all_but_last_by_mut_ref(parent: &mut A, x: &mut vector<Receiving<B>>) {
        while (vector::length(x) > 1) {
            let r = vector::pop_back(x);
            let b = transfer::receive(&mut parent.id, r);
            transfer::public_transfer(b, object::id_address(parent));
        };
    }

    public fun receive_all_by_mut_ref(parent: &mut A, x: &mut vector<Receiving<B>>) {
        while (!vector::is_empty(x)) {
            let r = vector::pop_back(x);
            let b = transfer::receive(&mut parent.id, r);
            transfer::public_transfer(b, object::id_address(parent));
        };
    }

    public fun receive_all_but_last(parent: &mut A, mut x: vector<Receiving<B>>) {
        while (vector::length(&x) > 1) {
            let r = vector::pop_back(&mut x);
            let b = transfer::receive(&mut parent.id, r);
            transfer::public_transfer(b, object::id_address(parent));
        };
    }

    public fun receive_all_send_back(parent: &mut A, mut x: vector<Receiving<B>>) {
        while (!vector::is_empty(&x)) {
            let r = vector::pop_back(&mut x);
            let b = transfer::receive(&mut parent.id, r);
            transfer::public_transfer(b, object::id_address(parent));
        };
    }

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

// Make the Move vec but then never use it -- this should be fine since they're all drop
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);

// Make the Move vec and pass, but never receive
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_none(Input(0), Result(0));

// Make the Move vec of receiving arguments and then receive all but the last. Only the ince we receive should be mutated
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all_but_last(Input(0), Result(0));

// Make the Move vec of receiving arguments, pass to a function by immref, then later use the vec to receive all of them
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_none_by_immref(Input(0), Result(0));
//> 2: tto::M1::receive_all_send_back(Input(0), Result(0));

// Make the Move vec of receiving arguments, pass to a function by mutref, then later use the vec to receive all of them
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_none_by_mutref(Input(0), Result(0));
//> 2: tto::M1::receive_all_send_back(Input(0), Result(0));

// Make the Move vec of receiving arguments, pass to a function by mutref and receive some
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all_but_last_by_mut_ref(Input(0), Result(0));
//> 2: tto::M1::receive_all_by_mut_ref(Input(0), Result(0));

// Make the Move vec of receiving arguments, pass to a function by mutref, receive some, then pass by mutref again to receive the rest
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all_but_last_by_mut_ref(Input(0), Result(0));
//> 2: tto::M1::receive_all_by_mut_ref(Input(0), Result(0));

// Make the Move vec of receiving arguments, pass to a function by mutref, receive some, then pass by value to receive the rest
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all_but_last_by_mut_ref(Input(0), Result(0));
//> 2: tto::M1::receive_all_send_back(Input(0), Result(0));

// Make the Move vec of receiving arguments and then receive all of them
//# programmable --inputs object(2,0) receiving(2,1) receiving(2,2) receiving(2,3) receiving(2,4)
//> 0: MakeMoveVec<sui::transfer::Receiving<tto::M1::B>>([Input(1), Input(2), Input(3), Input(4)]);
//> 1: tto::M1::receive_all(Input(0), Result(0));

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3

//# view-object 2,4
