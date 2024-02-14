// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid return values from public functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct A has copy, drop { value: u64 }
    public struct B has copy, drop { value: u256 }

    public fun t1(_: &mut u64): &mut u64 { abort 0}
    public fun t2(_: &mut u64): &u64 { abort 0}
    public fun t3(_: &mut u64): (u64, &u64) { abort 0}
}

//# programmable
//> 0: test::m1::t1();

//# programmable
//> 0: test::m1::t2();

//# programmable
//> 0: test::m1::t3();
