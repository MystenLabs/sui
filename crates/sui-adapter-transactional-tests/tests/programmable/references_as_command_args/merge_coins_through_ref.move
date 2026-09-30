// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }

//# programmable --sender A --inputs 1000 100 @B
// VALID: merge into a `&mut Coin<SUI>` result
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,0));
//> 2: MergeCoins(Result(1), [NestedResult(0,1)]);
//> 3: TransferObjects([NestedResult(0,0)], Input(2));

//# view-object 2,0
