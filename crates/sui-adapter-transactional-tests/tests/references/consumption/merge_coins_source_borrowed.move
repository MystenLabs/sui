// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 @A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, a coin merged into a reference to itself
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: MergeCoins(Result(1), [Result(0)]);
//> 3: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 1000 @A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, a coin merged into itself by value
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Result(0), [Result(0)]);
//> 2: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 1000 100 @A
// VALID: the source merged once its `&mut Balance` is dead, the target holds both amounts
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(NestedResult(0,1));
//> 2: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 3: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 4: TransferObjects([NestedResult(0,0)], Input(2));

//# view-object 4,0
