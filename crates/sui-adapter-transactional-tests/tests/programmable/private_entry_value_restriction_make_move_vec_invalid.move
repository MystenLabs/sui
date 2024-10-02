// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests MakeMoveVec makes a "dirty" value if at least one value has been used in a non-entry
// function used private entry function

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has key, store { id: UID }
    public fun r(ctx: &mut TxContext): R { R { id: object::new(ctx) } }

    public fun dirty(_: &mut R) {}
    public fun dirty_u64(_: &mut u64) {}
    entry fun priv(_: vector<R>) { abort 0 }
    entry fun priv2(_: vector<u64>) { abort 0 }
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> 1: test::m1::r();
//> 2: test::m1::r();
//> TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# programmable --sender A --inputs object(2,0) object(2,1) object(2,2)
//> 0: test::m1::dirty(Input(2));
//> 1: MakeMoveVec([Input(0), Input(1), Input(2)]);
//> test::m1::priv(Result(1))

//# programmable --sender A --inputs 0 0 0
//> 0: test::m1::dirty_u64(Input(1));
//> 1: MakeMoveVec<u64>([Input(0), Input(1), Input(2)]);
//> test::m1::priv(Result(1))
