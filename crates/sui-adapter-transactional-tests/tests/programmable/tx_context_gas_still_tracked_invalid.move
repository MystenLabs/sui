// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Per-command TxContext rooting must not weaken GasCoin tracking: while a `&mut Balance<SUI>`
// rooted in the gas coin (returned from a ctx-taking call) is live, borrowing the gas
// coin again via SplitCoins is rejected.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::balance::Balance;
use sui::coin::Coin;
use sui::sui::SUI;

public fun bal_mut(c: &mut Coin<SUI>, _ctx: &mut TxContext): &mut Balance<SUI> {
    c.balance_mut()
}

public fun check_positive(b: &Balance<SUI>, _ctx: &TxContext) {
    assert!(b.value() > 0, 0);
}

//# programmable --sender A --inputs 100 @A
// splitting the gas coin while the `&mut Balance<SUI>` result is live must fail
//> 0: test::m::bal_mut(Gas);
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: test::m::check_positive(Result(0));
//> TransferObjects([Result(1)], Input(1))
