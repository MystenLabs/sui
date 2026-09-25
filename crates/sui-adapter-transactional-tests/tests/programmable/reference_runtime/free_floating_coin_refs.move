// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun new_mut(): &mut Coin<SUI> { abort 0 }

//# programmable --sender A --inputs 100 @A
// ABORTS: a free-floating `&mut Coin` is writable, the write goes through `is_writable` with no borrowers; the callee aborts at runtime
//> 0: test::m::new_mut();
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# programmable --sender A --inputs 100 @A
// ABORTS: the same through MergeCoins
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::new_mut();
//> 2: MergeCoins(Result(1), [Result(0)]);
