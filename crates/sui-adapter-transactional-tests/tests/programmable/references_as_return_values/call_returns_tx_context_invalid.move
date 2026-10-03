// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `TxContext` is never a source of a returned reference: a reference obtained through a
// `TxContext` parameter roots only in the other arguments, or in nothing.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun digest_via(ctx: &TxContext, _x: &u64): &vector<u8> { ctx.digest() }

public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}

//# programmable --inputs 0
// INVALID: InvalidReferenceArgument at arg 0 of command 1, the laundered reference blocks a `&mut` of the other argument
//> 0: test::m::digest_via(Input(0));
//> 1: test::m::use_mut<u64>(Input(0));
//> 2: test::m::use_imm<vector<u8>>(Result(0));
