// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various vector instantions with objects

//# init --addresses test=0x0 --accounts A B

//# publish
module test::m1 {

    public struct Pub has key, store {
        id: UID,
        value: u64,
    }

    public struct Cap {}

    public struct Cup<T> has key, store {
        id: UID,
        value: T,
    }

    public fun new(ctx: &mut TxContext): Pub {
        Pub { id: object::new(ctx), value: 112 }
    }

    public fun cup<T>(value: T, ctx: &mut TxContext): Cup<T> {
        Cup { id: object::new(ctx), value }
    }

    public fun cap(): Cap {
        Cap {}
    }

    public fun pubs(mut v: vector<Pub>) {
        while (!vector::is_empty(&v)) {
            let Pub { id, value: _ } = vector::pop_back(&mut v);
            object::delete(id);
        };
        vector::destroy_empty(v);
    }
}

// objects
//# programmable --sender A
//> 0: test::m1::new();
//> 1: test::m1::new();
//> 2: test::m1::new();
//> 3: MakeMoveVec([Result(0), Result(1), Result(2)]);
//> test::m1::pubs(Result(3));

// annotated objects
//# programmable --sender A
//> 0: test::m1::new();
//> 1: test::m1::new();
//> 2: test::m1::new();
//> 3: MakeMoveVec<test::m1::Pub>([Result(0), Result(1), Result(2)]);
//> test::m1::pubs(Result(3));

// empty objects
//# programmable --sender A
//> 0: MakeMoveVec<test::m1::Pub>([]);
//> test::m1::pubs(Result(0));

// mixed new and old. Send an object to A and mix it in a vector with the newly created ones.
//# programmable --sender A --inputs @A
//> 0: test::m1::new();
//> TransferObjects([Result(0)], Input(0));

//# view-object 5,0

//# programmable --sender A --inputs object(5,0)
//> 0: test::m1::new();
//> 1: test::m1::new();
//> 2: test::m1::new();
// use Input and new objects
//> 3: MakeMoveVec([Result(0), Result(1), Input(0), Result(2)]);
//> test::m1::pubs(Result(3));
