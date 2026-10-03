// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

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

//# programmable --sender A --inputs 0 9 @A
// INVALID: InvalidReferenceArgument at arg 0 of command 6, pop while the element chain is live
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: MakeMoveVec<test::m::Obj>([Result(0), Result(1)]);
//> 3: std::vector::borrow_mut<test::m::Obj>(Result(2), Input(0));
//> 4: test::m::inner_mut(Result(3));
//> 5: test::m::f_mut(Result(4));
//> 6: std::vector::pop_back<test::m::Obj>(Result(2));
//> 7: test::m::write(Result(5), Input(1));
//> 8: std::vector::pop_back<test::m::Obj>(Result(2));
//> 9: std::vector::destroy_empty<test::m::Obj>(Result(2));
//> 10: TransferObjects([Result(6), Result(8)], Input(2));
