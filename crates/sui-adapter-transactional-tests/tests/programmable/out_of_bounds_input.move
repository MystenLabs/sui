// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests out of bounds arguments for Input

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun copy_(_: u64) { abort 0 }
    public fun take(_: R) { abort 0 }
    public fun by_imm(_: &u64) { abort 0 }
    public fun by_mut(_: &mut u64) { abort 0 }
}

//# programmable
//> test::m1::copy_(Input(0))
//# programmable --inputs 0
//> test::m1::copy_(Input(1))

//# programmable
//> test::m1::take(Input(0))
//# programmable --inputs 0
//> test::m1::take(Input(2))

//# programmable
//> test::m1::by_imm(Input(0))
//# programmable --inputs 0
//> test::m1::by_imm(Input(1))

//# programmable
//> test::m1::by_mut(Input(0))
//# programmable --inputs 0
//> test::m1::by_mut(Input(1))

//# programmable
//> MakeMoveVec([Input(0)])
//# programmable --inputs 0
//> MakeMoveVec<u64>([Input(0), Input(1)])

//# programmable
//> SplitCoins(Input(0), [Gas])
//# programmable --inputs 0
//> SplitCoins(Gas, [Input(1)])

//# programmable
//> MergeCoins(Input(0), [Gas])
//# programmable --inputs 0
//> MergeCoins(Gas, [Input(1), Input(0)])

//# programmable
//> TransferObjects([Input(0)], Gas)
//# programmable --inputs 0
//> TransferObjects([Gas], Input(1))
