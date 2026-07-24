// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, a `&mut Coin<SUI>` result from a ctx-taking call can
// be used as the coin argument of SplitCoins after a mutable TxContext use. SplitCoins
// consumes its coin argument through the write_ref path (the reference must be writable),
// which is stricter than the call-argument path.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut(c: &mut Coin<SUI>, _ctx: &mut TxContext): &mut Coin<SUI> {
    c
}

public fun mut_tx(_: &mut TxContext) {
}

//# programmable --sender A --inputs 100 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut(Result(0));
//> 2: test::m::mut_tx();
//> 3: SplitCoins(Result(1), [Input(1)]);
//> 4: MergeCoins(Result(1), [Result(3)]);
//> TransferObjects([Result(0)], Input(2))
