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
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun f_mut_g(i: &mut Inner): (&mut u64, &u64) { (&mut i.f, &i.g) }
public fun two_mut(a: &mut u64, b: &mut u64) { *a = *a + 1; *b = *b + 1 }
public fun mut_imm(a: &mut u64, b: &u64) { *a = *b }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 1
// VALID: two `&mut` siblings from one call passed together, both written
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::two_mut(NestedResult(2,0), NestedResult(2,1));
//> 4: test::m::check(NestedResult(2,0), Input(0));
//> 5: test::m::check(NestedResult(2,1), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 0
// VALID: a `&mut` and a `&` sibling from one call passed together
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut_g(Result(1));
//> 3: test::m::mut_imm(NestedResult(2,0), NestedResult(2,1));
//> 4: test::m::check(NestedResult(2,0), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: `&mut` references into two different objects passed together
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::f_mut(Result(2));
//> 5: test::m::f_mut(Result(3));
//> 6: test::m::two_mut(Result(4), Result(5));
//> 7: test::m::check(Result(4), Input(0));
//> 8: test::m::delete(Result(0));
//> 9: test::m::delete(Result(1));
