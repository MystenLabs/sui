// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 100 @B 7 1
// VALID: SplitCoins, MergeCoins, TransferObjects, MakeMoveVec of unrelated values while a reference is live
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: SplitCoins(Gas, [Input(1), Input(1)]);
//> 3: MergeCoins(NestedResult(2,0), [NestedResult(2,1)]);
//> 4: TransferObjects([NestedResult(2,0)], Input(2));
//> 5: MakeMoveVec<u64>([Input(4), Input(4)]);
//> 6: test::m::write(Result(1), Input(3));

//# view-object 2,0

//# programmable --sender A --inputs 100 @B
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, a coin merged away while borrowed
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,1));
//> 2: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 3: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([NestedResult(0,0)], Input(1));

//# programmable --sender A --inputs 100 @B
// INVALID: CannotWriteToExtendedReference at arg 0 of command 2, a coin merged into while an extension is live
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> 1: sui::coin::balance_mut<sui::sui::SUI>(NestedResult(0,0));
//> 2: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 3: test::m::use_mut<sui::balance::Balance<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([NestedResult(0,0)], Input(1));

//# programmable --sender A --inputs 100 @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, a coin transferred while borrowed
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: TransferObjects([Result(0)], Input(1));
//> 3: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, an object put in a vector while borrowed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: MakeMoveVec([Input(0)]);
//> 3: test::m::use_mut<u64>(Result(1));
