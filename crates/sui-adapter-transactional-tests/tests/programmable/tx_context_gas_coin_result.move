// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, a `&mut Balance<SUI>` returned from a ctx-taking
// call on the gas coin roots in the GasCoin location, not in the injected TxContext,
// and stays valid across a later mutable TxContext use.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::balance::Balance;
use sui::coin::Coin;
use sui::sui::SUI;

public fun bal_mut(c: &mut Coin<SUI>, _ctx: &mut TxContext): &mut Balance<SUI> {
    c.balance_mut()
}

public fun mut_tx(_: &mut TxContext) {
}

public fun check_positive(b: &Balance<SUI>, _ctx: &TxContext) {
    assert!(b.value() > 0, 0);
}

//# programmable
//> 0: test::m::bal_mut(Gas);
//> 1: test::m::mut_tx();
//> 2: test::m::check_positive(Result(0));
