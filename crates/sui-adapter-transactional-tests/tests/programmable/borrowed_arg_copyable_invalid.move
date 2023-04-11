// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various invalid usages of a borrowed arg

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public fun imm_and_mut(_: &u64, _: &mut u64) { abort 0 }
    public fun mut_and_imm(_: &mut u64, _: &u64) { abort 0 }

    public fun imm_mut_imm(_:&u64, _: &mut u64, _: &u64) { abort 0 }
    public fun imm_copy_mut(_: &u64, _: u64, _: &mut u64) { abort 0 }
}

//# programmable --inputs 0
//> test::m1::imm_and_mut(Input(0), Input(0))

//# programmable --inputs 0
//> test::m1::mut_and_imm(Input(0), Input(0))

//# programmable --inputs 0
//> test::m1::imm_mut_imm(Input(0), Input(0), Input(0))

//# programmable --inputs 0
//> test::m1::imm_copy_mut(Input(0), Input(0), Input(0))
