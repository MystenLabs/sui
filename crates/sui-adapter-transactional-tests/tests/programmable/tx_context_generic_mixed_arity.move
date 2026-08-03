// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A generic TxContext slot is injected at its declared position among real
// parameters, whether it leads or trails the user arguments; supplying it
// explicitly on top of the real arguments is an arity error.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun tx_context_mut_id(ctx: &mut TxContext): &mut TxContext {
    ctx
}

public fun gen_leading<T>(x: &mut T, _y: u64): &mut T {
    x
}

public fun gen_trailing<T>(_x: u64, y: &mut T): &mut T {
    y
}

//# programmable --inputs 0
// only the u64 is supplied; the leading &mut T (TxContext) is injected
//> test::m::gen_leading<sui::tx_context::TxContext>(Input(0));

//# programmable --inputs 0
// only the u64 is supplied; the trailing &mut T (TxContext) is injected
//> test::m::gen_trailing<sui::tx_context::TxContext>(Input(0));

//# programmable --inputs 0
// supplying both the u64 and a TxContext result is one arg too many
//> 0: test::m::tx_context_mut_id();
//> test::m::gen_trailing<sui::tx_context::TxContext>(Input(0), Result(0));
