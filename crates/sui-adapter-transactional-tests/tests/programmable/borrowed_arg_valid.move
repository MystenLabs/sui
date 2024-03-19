// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various usages of a borrowed arg

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public struct R has drop {}
    public fun r(): R { R {} }

    public fun imm_copy(_: &u64, _: u64,) {}
    public fun copy_imm(_: u64, _: &u64) {}

    public fun copy_mut(_: u64, _: &mut u64) {}
    public fun mut_copy(_: &mut u64, _: u64) {}
    public fun copy_mut_copy(_: u64, _: &mut u64, _: u64) {}

    public fun multiple_copy(_: &u64, _: u64, _: &u64, _: u64) {}


    public fun double_r(_: &R, _: &R) {}
}

//# programmable --inputs 0
// imm borrow and copy
//> test::m1::copy_imm(Input(0), Input(0));
//> test::m1::copy_imm(Input(0), Input(0));
// can copy even after being mutably borrowed
//> test::m1::copy_mut(Input(0), Input(0));
//> test::m1::mut_copy(Input(0), Input(0));
//> test::m1::copy_mut_copy(Input(0), Input(0), Input(0));
// mix all and borrow multiple times
//> test::m1::multiple_copy(Input(0), Input(0), Input(0), Input(0));

//# programmable
//> 0: test::m1::r();
// double borrow without copy
//> test::m1::double_r(Result(0), Result(0))
