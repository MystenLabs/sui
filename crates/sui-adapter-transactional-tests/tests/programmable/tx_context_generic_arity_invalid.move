// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// verifies Todd's PTB arity example: an identity function that returns
// &mut TxContext cannot be chained into a generic identity function whose
// type parameter unifies to TxContext, because auto-injection fills the
// generic's slot after unification. The tx_context_restrictions_verifier
// only applies to system packages, so this user package publishes fine;
// the PTB arity check is the safety mechanism here.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun tx_context_mut_id(ctx: &mut TxContext): &mut TxContext {
    ctx
}

public fun tx_context_imm_id(ctx: &TxContext): &TxContext {
    ctx
}

public fun gen_mut_id<T>(x: &mut T): &mut T {
    x
}

public fun gen_imm_id<T>(x: &T): &T {
    x
}

//# programmable
// Result(0) is `&mut TxContext`. gen_mut_id<TxContext>'s single &mut T slot
// is auto-injected, so passing Result(0) is one arg too many.
//> 0: test::m::tx_context_mut_id();
//> test::m::gen_mut_id<sui::tx_context::TxContext>(Result(0));

//# programmable
// the same shape through immutable references
//> 0: test::m::tx_context_imm_id();
//> test::m::gen_imm_id<sui::tx_context::TxContext>(Result(0));

//# programmable
// a generic identity's own result cannot be fed back into it either
//> 0: test::m::gen_mut_id<sui::tx_context::TxContext>();
//> test::m::gen_mut_id<sui::tx_context::TxContext>(Result(0));
