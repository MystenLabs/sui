// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid multiple &mut/&TxContext parameters

//# init --addresses test=0x0

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

public fun mut_u64_mut(_: &mut TxContext, _: u64, _: &mut TxContext) {
}

public fun mut_imm_mut(_: &mut TxContext, _: &TxContext, _: &mut TxContext) {
}

public fun gen_mut_mut<T>(_: &mut T, _: &mut TxContext) {
}

public fun gen_mut_imm<T>(_: &mut T, _: &TxContext) {
}

public fun gen_imm_mut<T>(_: &T, _: &mut TxContext) {
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

//# programmable --inputs 0
// the duplicate mutable usage is reported at the second &mut TxContext
//> test::m::mut_u64_mut(Input(0));

//# programmable
// both violations at once: the duplicate mutable usage is reported, not the mixed usage
//> test::m::mut_imm_mut();

//# programmable
// a generic &mut T unifying to &mut TxContext alongside a concrete &mut TxContext is two mutable
// usages
//> test::m::gen_mut_mut<sui::tx_context::TxContext>();

//# programmable
// a generic &mut T unifying to &mut TxContext alongside a concrete &TxContext mixes mutable and
// immutable usages
//> test::m::gen_mut_imm<sui::tx_context::TxContext>();

//# programmable
// a generic &T unifying to &TxContext alongside a concrete &mut TxContext mixes mutable and
// immutable usages
//> test::m::gen_imm_mut<sui::tx_context::TxContext>();
