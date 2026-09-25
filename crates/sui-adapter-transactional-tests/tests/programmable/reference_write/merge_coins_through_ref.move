// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }

//# programmable --sender A --inputs 1000 100 @B
// VALID: merge into a `&mut Coin<SUI>` result
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,0));
//> 2: MergeCoins(Result(1), [NestedResult(0,1)]);
//> 3: TransferObjects([NestedResult(0,0)], Input(2));

//# view-object 2,0

//# programmable --sender A --inputs 1000 100 @B
// INVALID: TypeMismatch at arg 0 of command 2, `&Coin` target
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,0));
//> 2: MergeCoins(Result(1), [NestedResult(0,1)]);
//> 3: TransferObjects([NestedResult(0,0)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: TypeMismatch at arg 1 of command 2, `&Coin` source
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,1));
//> 2: MergeCoins(NestedResult(0,0), [Result(1)]);
//> 3: TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: TypeMismatch at arg 1 of command 2, `&mut Coin` source
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,1));
//> 2: MergeCoins(NestedResult(0,0), [Result(1)]);
//> 3: TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2));
