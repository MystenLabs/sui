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
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun set_inner(i: &mut Inner, f: u64, g: u64) { i.f = f; i.g = g }
public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 1 2
// VALID: depth one
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::set_inner(Result(0), Input(1), Input(2));

//# view-object 2,0

//# programmable --sender A --inputs object(2,0) 3 4
// VALID: depth two, both fields through sibling references
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_g_mut(Result(0));
//> 2: test::m::write(NestedResult(1,0), Input(1));
//> 3: test::m::write(NestedResult(1,1), Input(2));

//# view-object 2,0
