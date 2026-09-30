// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }

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
