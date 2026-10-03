// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 1, split while a balance reference is live
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(0));
