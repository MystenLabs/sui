// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun new_mut(): &mut Coin<SUI> { abort 0 }

//# programmable --sender A --inputs 100 @A
// ABORTS: SplitCoins passes static checking with a free-floating reference, but new_mut aborts with code 0 before the split
//> 0: test::m::new_mut();
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# programmable --sender A --inputs 100 @A
// ABORTS: MergeCoins passes static checking with a free-floating reference, but new_mut aborts with code 0 before the merge
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::new_mut();
//> 2: MergeCoins(Result(1), [Result(0)]);
