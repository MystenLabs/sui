// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that non-input primitive values can be used private entry functions, even if they have
// been copied to other, non-entry functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public fun clean(_: u64) {}
    entry fun priv(_: u64) {}
}

//# programmable --inputs 0
//> test::m1::clean(Input(0));
//> test::m1::priv(Input(0));
