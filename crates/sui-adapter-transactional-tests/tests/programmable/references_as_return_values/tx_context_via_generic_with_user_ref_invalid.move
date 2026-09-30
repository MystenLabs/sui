// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun two_mut_then_ref<T>(_: &mut T, _: &mut T, r: &mut u64): &mut u64 { r }
public fun mut_imm_then_ref<T>(_: &mut T, _: &T, r: &mut u64): &mut u64 { r }

public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable --sender A --inputs 1 7
// INVALID: two injected `&mut TxContext` through one generic, rejected like the non-generic case
//> 0: test::m::two_mut_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# programmable --sender A --inputs 1 7
// INVALID: injected `&mut TxContext` next to an injected `&TxContext` through one generic
//> 0: test::m::mut_imm_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
