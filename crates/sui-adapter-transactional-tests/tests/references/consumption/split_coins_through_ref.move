// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }

//# programmable --sender A --inputs 1000 100 @B
// VALID: split through a `&mut Coin<SUI>` result
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: SplitCoins(Result(1), [Input(1)]);
//> 3: TransferObjects([Result(0), Result(2)], Input(2));

//# view-object 2,0

//# view-object 2,1

//# programmable --sender A --inputs 1000 100 @B
// VALID: split through a `&mut` result twice, then through the value
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: SplitCoins(Result(1), [Input(1)]);
//> 3: SplitCoins(Result(1), [Input(1)]);
//> 4: SplitCoins(Result(0), [Input(1)]);
//> 5: TransferObjects([Result(0), Result(2), Result(3), Result(4)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: TypeMismatch at arg 0 of command 2, `&Coin` is not writable
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: SplitCoins(Result(1), [Input(1)]);
//> 3: TransferObjects([Result(0), Result(2)], Input(2));
