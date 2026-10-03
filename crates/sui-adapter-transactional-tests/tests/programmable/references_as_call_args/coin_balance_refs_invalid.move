// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun use_imm<T>(_: &T) {}

//# programmable --sender A --inputs 1000 100 @B
// INVALID: InvalidReferenceArgument at arg 0 of command 3, join while a `&Balance` is live
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: sui::coin::balance<sui::sui::SUI>(NestedResult(0,0));
//> 2: sui::coin::value<sui::sui::SUI>(NestedResult(0,0));
//> 3: sui::coin::join<sui::sui::SUI>(NestedResult(0,0), NestedResult(0,1));
//> 4: test::m::use_imm<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 5: TransferObjects([NestedResult(0,0)], Input(2));
