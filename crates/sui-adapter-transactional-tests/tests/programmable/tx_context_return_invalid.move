// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TxContext in return position of Move calls; it can never become a PTB result

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public fun make(): TxContext {
    abort 0
}

public fun make_trailing(): (u64, TxContext) {
    abort 0
}

public fun gen_make<T>(): T {
    abort 0
}

public fun ref_mut_id(ctx: &mut TxContext): &mut TxContext {
    ctx
}

public fun ref_imm_id(ctx: &TxContext): &TxContext {
    ctx
}

//# programmable
// TxContext cannot be returned by value
//> test::m::make();

//# programmable
// TxContext cannot be returned by value, at any position
//> test::m::make_trailing();

//# programmable
// TxContext cannot be returned, even via generic instantiation
//> test::m::gen_make<sui::tx_context::TxContext>();

//# programmable --sender A --dev-inspect
// TxContext cannot be returned, even in dev-inspect
//> test::m::make();

//# programmable
// rejected by the general no-references-in-return rule
//> test::m::ref_mut_id();

//# programmable --sender A --dev-inspect
// dev-inspect allows reference returns in general, but not of TxContext
//> test::m::ref_mut_id();

//# programmable --sender A --dev-inspect
// dev-inspect allows reference returns in general, but not of TxContext
//> test::m::ref_imm_id();
