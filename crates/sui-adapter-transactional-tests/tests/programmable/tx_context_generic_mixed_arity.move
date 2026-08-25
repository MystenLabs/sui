// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A generic TxContext slot is injected at its declared position among real
// parameters, whether it leads or trails the user arguments; supplying an
// extra argument on top of the real arguments is an arity error.

//# init --addresses test=0x0

//# publish
module test::m;

public fun gen_leading<T>(_x: &mut T, _y: u64) {
}

public fun gen_trailing<T>(_x: u64, _y: &mut T) {
}

//# programmable --inputs 0
// only the u64 is supplied; the leading &mut T (TxContext) is injected
//> test::m::gen_leading<sui::tx_context::TxContext>(Input(0));

//# programmable --inputs 0
// only the u64 is supplied; the trailing &mut T (TxContext) is injected
//> test::m::gen_trailing<sui::tx_context::TxContext>(Input(0));

//# programmable --inputs 0 0
// supplying an argument for the injected TxContext slot is one arg too many
//> test::m::gen_trailing<sui::tx_context::TxContext>(Input(0), Input(1));
