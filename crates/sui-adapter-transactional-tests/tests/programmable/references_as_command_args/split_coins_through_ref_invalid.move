// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id<T>(t: &T): &T { t }

//# programmable --sender A --inputs 1000 100 @B
// INVALID: TypeMismatch at arg 0 of command 2, `&Coin` is not writable
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: SplitCoins(Result(1), [Input(1)]);
//> 3: TransferObjects([Result(0), Result(2)], Input(2));
