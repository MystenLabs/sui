// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `&mut TxContext` still cannot coexist with another TxContext borrow in the same call.
// Mirrors the flag-off tx_context_multiple_borrows_invalid.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun mut_mut(_: &mut TxContext, _: &mut TxContext) {
}

public fun mut_imm(_: &mut TxContext, _: &TxContext) {
}

public fun imm_mut(_: &TxContext, _: &mut TxContext) {
}

public fun imm_u64_mut(_: &TxContext, _: u64, _: &mut TxContext) {
}

public fun mut_u64_imm(_: &mut TxContext, _: u64, _: &TxContext) {
}


//# programmable
//> test::m::mut_mut();

//# programmable
//> test::m::mut_imm();

//# programmable
//> test::m::imm_mut();

//# programmable --inputs 0
//> test::m::imm_u64_mut(Input(0));

//# programmable --inputs 0
//> test::m::mut_u64_imm(Input(0));
