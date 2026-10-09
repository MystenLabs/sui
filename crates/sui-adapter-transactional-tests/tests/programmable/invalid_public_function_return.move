// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests reference return values from public functions, used to be disallowed

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct A has copy, drop { value: u64 }
    public struct B has copy, drop { value: u256 }

    public fun t1(x: &mut u64): &mut u64 { (x) }
    public fun t2(x: &mut u64): &u64 { (x) }
    public fun t3(x: &mut u64): (u64, &u64) { (*x, x) }
}

//# programmable --inputs 0
//> 0: test::m1::t1(Input(0));

//# programmable --inputs 0
//> 0: test::m1::t2(Input(0));

//# programmable --inputs 0
//> 0: test::m1::t3(Input(0));
