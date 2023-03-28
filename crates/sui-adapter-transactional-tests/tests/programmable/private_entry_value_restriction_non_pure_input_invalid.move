// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that non-input primitive values cannot be used private entry functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public fun v1(): u64 { 0 }
    entry fun v2(): u64 { 0 }
    public fun dirty(_: &mut u64) {}
    entry fun priv(_: u64) { abort 0 }
}

// cannot use results from other functions
//# programmable
//> 0: test::m1::v1();
//> test::m1::priv(Result(0));

//# programmable
//> 0: test::m1::v2();
//> test::m1::priv(Result(0));

// pure value has been "dirtied" and cannot be used
//# programmable --inputs 0
//> test::m1::dirty(Input(0));
//> test::m1::priv(Input(0));
