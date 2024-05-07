// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests dirty check happens in order of arguments, nto all at once

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has key, store { id: UID }
    public fun r(ctx: &mut TxContext): R { R { id: object::new(ctx) } }

    public fun dirty(_: &mut R) {}
    entry fun priv(_: &mut R, _: &mut R) {}
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> 1: test::m1::r();
//> TransferObjects([Result(0), Result(1)], Input(0))

//# programmable --sender A --inputs object(2,0) object(2,1)
//> test::m1::dirty(Input(1));
//> test::m1::priv(Input(0), Input(1));

//# programmable --sender A --inputs 0u64 object(2,1)
//> test::m1::dirty(Input(1));
// type error instead of dirty error
//> test::m1::priv(Input(0), Input(1));
