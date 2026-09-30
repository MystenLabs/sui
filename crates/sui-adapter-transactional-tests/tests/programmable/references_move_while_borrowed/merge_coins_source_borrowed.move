// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 100 @A
// VALID: the source merged once its `&mut Balance` is dead, the target holds both amounts
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(NestedResult(0,1));
//> 2: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 3: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 4: TransferObjects([NestedResult(0,0)], Input(2));

//# view-object 2,0
