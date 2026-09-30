// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

//# programmable --sender A --inputs 1000 100 @B
// VALID: balance operations through the reference, then the coin split once the reference is dead
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(Result(0));
//> 2: sui::balance::split<sui::sui::SUI>(Result(1), Input(1));
//> 3: sui::balance::join<sui::sui::SUI>(Result(1), Result(2));
//> 4: SplitCoins(Result(0), [Input(1)]);
//> 5: TransferObjects([Result(0), Result(4)], Input(2));

//# view-object 2,0

//# view-object 2,1
