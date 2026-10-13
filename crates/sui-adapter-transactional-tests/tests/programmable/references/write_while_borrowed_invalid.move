// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// SplitCoins and MergeCoins reject a coin argument that has a live extension, mutable or
// immutable, with CannotWriteToExtendedReference.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun amount_ref(_: &Coin<SUI>, x: &u64): &u64 { x }

//# programmable --sender A --inputs 1000 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 1, the gas coin split while a `&mut Balance` into it is live
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: SplitCoins(Gas, [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 2, a coin result split while a `&mut Balance` into it is live
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(Result(0));
//> 2: SplitCoins(Result(0), [Input(1)]);
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([Result(0), Result(2)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 3, split through a `&mut Coin` result with a live `&mut Balance` extension
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: sui::coin::balance_mut<sui::sui::SUI>(Result(1));
//> 3: SplitCoins(Result(1), [Input(1)]);
//> 4: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(2));
//> 5: TransferObjects([Result(0), Result(3)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 3, an immutable `&Balance` extension also blocks the write
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: sui::coin::balance<sui::sui::SUI>(Result(1));
//> 3: SplitCoins(Result(1), [Input(1)]);
//> 4: test::m::use_imm<sui::balance::Balance<sui::sui::SUI>>(Result(2));
//> 5: TransferObjects([Result(0), Result(3)], Input(2));

//# programmable --sender A --inputs 1000 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 3, merge through a `&mut Coin` result with a live extension
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,0));
//> 2: sui::coin::balance_mut<sui::sui::SUI>(Result(1));
//> 3: MergeCoins(Result(1), [NestedResult(0,1)]);
//> 4: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(2));
//> 5: TransferObjects([NestedResult(0,0)], Input(2));

//# programmable --sender A --inputs 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 2, a coin merged into while an extension is live
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(NestedResult(0,0));
//> 2: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([NestedResult(0,0)], Input(1));

//# programmable --sender A --inputs 100 @A
// INVALID: CannotWriteToExtendedReference at arg 0 of command 1, the coin-rooted amount reference is used again after the split, so its read is a copy
//> 0: test::m::amount_ref(Gas, Input(0));
//> 1: SplitCoins(Gas, [Result(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::use_imm<u64>(Result(0));
