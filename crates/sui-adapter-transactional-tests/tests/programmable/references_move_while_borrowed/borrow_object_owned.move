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

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 10 @B
// VALID: the input is transferred by value after the reference into it is dead
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: TransferObjects([Input(0)], Input(2));

//# view-object 2,0
