// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 2, SplitCoins while a `&mut Balance` is live
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(Result(0));
//> 2: SplitCoins(Result(0), [Input(1)]);
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([Result(0), Result(2)], Input(2));
