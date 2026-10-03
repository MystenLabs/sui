// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun amount_ref(_: &Coin<SUI>, x: &u64): &u64 { x }

//# programmable --sender A --inputs 100 @A
// VALID: an amount read through a reference rooted in the split coin is released before the coin is written
//> 0: test::m::amount_ref(Gas, Input(0));
//> 1: SplitCoins(Gas, [Result(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
