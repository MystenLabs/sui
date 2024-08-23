// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests  NestedResult(0,0) is always equivalent to Result(0)

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has copy, drop {}
    public fun r(): R { R{} }
    public fun copy_(_: R) {}
}

//# programmable
//> 0: test::m1::r();
//> test::m1::copy_(Result(0));
//> test::m1::copy_(NestedResult(0, 0));
