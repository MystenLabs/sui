// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 100 @B
// INVALID: InvalidReferenceArgument at arg 0 of command 3, join while a `&Balance` is live
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: sui::coin::balance<sui::sui::SUI>(NestedResult(0,0));
//> 2: sui::coin::value<sui::sui::SUI>(NestedResult(0,0));
//> 3: sui::coin::join<sui::sui::SUI>(NestedResult(0,0), NestedResult(0,1));
//> 4: test::m::use_imm<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 5: TransferObjects([NestedResult(0,0)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 2, SplitCoins while a `&mut Balance` is live
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(Result(0));
//> 2: SplitCoins(Result(0), [Input(1)]);
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([Result(0), Result(2)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// VALID: balance operations through the reference, then the coin split once the reference is dead
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(Result(0));
//> 2: sui::balance::split<sui::sui::SUI>(Result(1), Input(1));
//> 3: sui::balance::join<sui::sui::SUI>(Result(1), Result(2));
//> 4: SplitCoins(Result(0), [Input(1)]);
//> 5: TransferObjects([Result(0), Result(4)], Input(2));

//# view-object 4,0

//# view-object 4,1
