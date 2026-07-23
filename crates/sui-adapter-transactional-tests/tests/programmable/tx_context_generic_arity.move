// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A generic reference parameter whose type parameter unifies to TxContext is
// auto-injected like a concrete TxContext parameter, so zero-argument calls
// succeed. A non-TxContext instantiation takes the normal user-argument path.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun gen_mut_id<T>(x: &mut T): &mut T {
    x
}

public fun gen_imm_id<T>(x: &T): &T {
    x
}

//# programmable
// &mut T unifies to &mut TxContext; the slot is auto-injected
//> test::m::gen_mut_id<sui::tx_context::TxContext>();

//# programmable
// &T unifies to &TxContext; the slot is auto-injected
//> test::m::gen_imm_id<sui::tx_context::TxContext>();

//# programmable --inputs 0
// T = u64 is not TxContext: no injection, the argument is supplied normally
//> test::m::gen_imm_id<u64>(Input(0));
