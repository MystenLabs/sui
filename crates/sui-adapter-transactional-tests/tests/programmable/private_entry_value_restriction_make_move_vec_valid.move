// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests MakeMoveVec does not "dirty" value if no value has been used in a non-entry
// function used private entry function

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {

    public struct R has key, store { id: UID }
    public fun r(ctx: &mut TxContext): R { R { id: object::new(ctx) } }

    public entry fun clean(_: &mut R) {}
    public entry fun clean_u64(_: u64) {}
    entry fun priv1(_: &mut R) {}
    entry fun priv2(mut v: vector<R>) {
        while (!vector::is_empty(&v)) {
            let R { id } = vector::pop_back(&mut v);
            object::delete(id);
        };
        vector::destroy_empty(v)
    }
    entry fun priv3(_: vector<u64>) {}
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> 1: test::m1::r();
//> 2: test::m1::r();
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# programmable --sender A --inputs object(2,0) object(2,1) object(2,2)
//> 0: test::m1::clean(Input(2));
//> 1: test::m1::priv1(Input(2));
//> 2: MakeMoveVec([Input(0), Input(1), Input(2)]);
//> test::m1::priv2(Result(2))

//# programmable --sender A --inputs 0 0 0
//> 0: test::m1::clean_u64(Input(1));
//> 1: MakeMoveVec<u64>([Input(0), Input(1), Input(2)]);
//> test::m1::priv3(Result(1))
