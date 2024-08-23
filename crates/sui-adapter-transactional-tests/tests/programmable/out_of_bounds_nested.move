// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests out of bounds arguments for NestedResult

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun r(): (R, R) { (R{}, R{}) }
    public fun copy_(_: u64) { abort 0 }
    public fun take(_: R) { abort 0 }
    public fun by_imm(_: &u64) { abort 0 }
    public fun by_mut(_: &mut u64) { abort 0 }
}

//# programmable
//> 0: test::m1::r();
//> test::m1::copy_(NestedResult(0, 2))
//# programmable
//> 0: test::m1::r();
//> test::m1::copy_(NestedResult(1, 0))

//# programmable
//> 0: test::m1::r();
//> test::m1::take(NestedResult(0, 2))
//# programmable
//> 0: test::m1::r();
//> test::m1::take(NestedResult(1, 0))

//# programmable
//> 0: test::m1::r();
//> test::m1::by_imm(NestedResult(0, 2))
//# programmable
//> 0: test::m1::r();
//> test::m1::by_imm(NestedResult(1, 0))

//# programmable
//> 0: test::m1::r();
//> test::m1::by_mut(NestedResult(0, 2))
//# programmable
//> 0: test::m1::r();
//> test::m1::by_mut(NestedResult(1, 0))

//# programmable
//> 0: test::m1::r();
//> MakeMoveVec([NestedResult(0, 2)])
//# programmable
//> 0: test::m1::r();
//> MakeMoveVec<u64>([NestedResult(0, 2), NestedResult(1, 0)])

//# programmable
//> 0: test::m1::r();
//> SplitCoins(NestedResult(0, 2), [Gas])
//# programmable
//> 0: test::m1::r();
//> SplitCoins(Gas, [NestedResult(1, 0)])

//# programmable
//> 0: test::m1::r();
//> MergeCoins(NestedResult(0, 2), [Gas])
//# programmable
//> 0: test::m1::r();
//> MergeCoins(Gas, [NestedResult(1, 0), NestedResult(0, 2)])

//# programmable
//> 0: test::m1::r();
//> TransferObjects([NestedResult(0, 2)], Gas)
//# programmable
//> 0: test::m1::r();
//> TransferObjects([Gas], NestedResult(1, 0))
