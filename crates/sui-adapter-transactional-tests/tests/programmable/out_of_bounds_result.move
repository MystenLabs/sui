// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests out of bounds arguments for Result

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun r(): R { R{} }
    public fun copy_(_: u64) { abort 0 }
    public fun take(_: R) { abort 0 }
    public fun by_imm(_: &u64) { abort 0 }
    public fun by_mut(_: &mut u64) { abort 0 }
}

//# programmable
//> test::m1::copy_(Result(0))
//# programmable
//> 0: test::m1::r();
//> test::m1::copy_(Result(1))

//# programmable
//> test::m1::take(Result(0))
//# programmable
//> 0: test::m1::r();
//> test::m1::take(Result(2))

//# programmable
//> test::m1::by_imm(Result(0))
//# programmable
//> 0: test::m1::r();
//> test::m1::by_imm(Result(1))

//# programmable
//> test::m1::by_mut(Result(0))
//# programmable
//> 0: test::m1::r();
//> test::m1::by_mut(Result(1))

//# programmable
//> MakeMoveVec([Result(0)])
//# programmable
//> 0: test::m1::r();
//> MakeMoveVec<u64>([Result(0), Result(1)])

//# programmable
//> SplitCoins(Result(0), [Gas])
//# programmable
//> 0: test::m1::r();
//> SplitCoins(Gas, [Result(1)])

//# programmable
//> MergeCoins(Result(0), [Gas])
//# programmable
//> 0: test::m1::r();
//> MergeCoins(Gas, [Result(1), Result(0)])

//# programmable
//> TransferObjects([Result(0)], Gas)
//# programmable
//> 0: test::m1::r();
//> TransferObjects([Gas], Result(1))
