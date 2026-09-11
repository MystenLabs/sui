// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Hot {}

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun f_mut_g(i: &mut Inner): (&mut u64, &u64) { (&mut i.f, &i.g) }
public fun f_g(i: &Inner): (&u64, &u64) { (&i.f, &i.g) }
public fun val_and_ref(i: &mut Inner): (u64, &mut u64) { (i.g, &mut i.f) }
public fun hot_and_ref(i: &mut Inner): (Hot, &mut u64) { (Hot {}, &mut i.f) }
public fun cool(h: Hot) { let Hot {} = h; }
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun mut_imm(_: &mut u64, _: &u64) {}
public fun two_imm(_: &u64, _: &u64) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun use_val(_: u64) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 1 2
// VALID: (mut, mut) written separately and passed together
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::write(NestedResult(2,1), Input(1));
//> 5: test::m::two_mut(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 1 0
// VALID: (mut, imm) written and read while both are live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut_g(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::check(NestedResult(2,1), Input(1));
//> 5: test::m::mut_imm(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 0
// VALID: (imm, imm)
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g(Result(1));
//> 3: test::m::check(NestedResult(2,0), Input(0));
//> 4: test::m::check(NestedResult(2,1), Input(0));
//> 5: test::m::two_imm(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 3
// VALID: (value, mut ref)
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::val_and_ref(Result(1));
//> 3: test::m::use_val(NestedResult(2,0));
//> 4: test::m::write(NestedResult(2,1), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable
// VALID: (hot potato, mut ref); the reference is dropped, the hot potato is consumed
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::hot_and_ref(Result(1));
//> 3: test::m::cool(NestedResult(2,0));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: UnusedValueWithoutDrop for result (2, 0), the hot potato next to a dropped reference
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::hot_and_ref(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidResultArity at arg 0 of command 3, Result on a multi-reference return
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: SecondaryIndexOutOfBounds at arg 0 of command 3
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(NestedResult(2,2), Input(0));
//> 4: test::m::delete(Result(0));
