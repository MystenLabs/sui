// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `TxContext` is never a source of a returned reference: a reference obtained through a
// `TxContext` parameter roots only in the other arguments, or in nothing.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun digest_via(ctx: &TxContext, _x: &u64): &vector<u8> { ctx.digest() }
public fun fresh(ctx: &mut TxContext): address { ctx.fresh_object_address() }
public fun eq_bytes(a: &vector<u8>, b: &vector<u8>) { assert!(a == b, 0) }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}

//# programmable
// VALID: a reference into TxContext survives later mutable TxContext uses and stays consistent
//> 0: sui::tx_context::digest();
//> 1: test::m::fresh();
//> 2: test::m::fresh();
//> 3: sui::tx_context::digest();
//> 4: test::m::eq_bytes(Result(0), Result(3));

//# programmable --inputs 0
// VALID: a laundered TxContext reference is attributed to the other argument, immutable uses are fine
//> 0: test::m::digest_via(Input(0));
//> 1: test::m::use_imm<u64>(Input(0));
//> 2: test::m::fresh();
//> 3: test::m::use_imm<vector<u8>>(Result(0));

//# programmable --inputs 0
// INVALID: InvalidReferenceArgument at arg 0 of command 1, the laundered reference blocks a `&mut` of the other argument
//> 0: test::m::digest_via(Input(0));
//> 1: test::m::use_mut<u64>(Input(0));
//> 2: test::m::use_imm<vector<u8>>(Result(0));
