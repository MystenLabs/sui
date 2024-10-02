// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests when NestedResult is needed, but Result is used

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun nop() {}
    public fun r(): (R, R) { (R{}, R{}) }
    public fun take(_: R) { abort 0 }
    public fun take_vec(_: vector<R>) { abort 0 }
}

//# programmable
//> 0: test::m1::nop();
//> test::m1::take(Result(0))
//# programmable
//> 0: test::m1::nop();
//> test::m1::take_vec(Result(0))

//# programmable
//> 0: test::m1::r();
//> test::m1::take(Result(0))
//# programmable
//> 0: test::m1::r();
//> test::m1::take_vec(Result(0))

//# programmable
//> 0: test::m1::r();
//> MakeMoveVec<test::m1::R>([Result(0)])
//# programmable
//> 0: test::m1::r();
//> MakeMoveVec<test::m1::R>([Result(0)])
