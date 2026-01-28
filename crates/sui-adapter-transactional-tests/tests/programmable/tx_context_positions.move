// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests various valid positions for TxContext parameters

//# init --addresses test=0x0

//# publish
module test::m;

public fun mut_0(_: &mut TxContext, _: u64, _: u64) {
}

public fun mut_1(_: u64, _: &mut TxContext, _: u64) {
}

public fun mut_2(_:u64, _: u64, _: &mut TxContext) {
}

public fun imm_0(_: &TxContext, _: u64, _: u64) {
}

public fun imm_1(_: u64, _: &TxContext, _: u64) {
}

public fun imm_2(_:u64, _: u64, _: &TxContext) {
}

//# programmable --inputs 0 0
//> test::m::mut_0(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::mut_1(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::mut_2(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::imm_0(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::imm_1(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::imm_2(Input(0), Input(1));

//# programmable --inputs 0 0
//> test::m::mut_0(Input(0), Input(1));
//> test::m::mut_1(Input(0), Input(1));
//> test::m::mut_2(Input(0), Input(1));
//> test::m::imm_0(Input(0), Input(1));
//> test::m::imm_1(Input(0), Input(1));
//> test::m::imm_2(Input(0), Input(1));
