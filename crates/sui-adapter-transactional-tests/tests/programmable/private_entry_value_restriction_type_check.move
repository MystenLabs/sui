// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests dirty check happens before type checking

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has key, store { id: UID }
    public fun r(ctx: &mut TxContext): R { R { id: object::new(ctx) } }

    public fun dirty(_: &mut R) {}
    entry fun priv1(_: u64) {}
}

//# programmable --sender A --inputs @A
//> 0: test::m1::r();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: test::m1::dirty(Input(0));
//> test::m1::priv1(Input(0));

//# programmable --sender A --inputs object(2,0)
//> test::m1::priv1(Input(0));
