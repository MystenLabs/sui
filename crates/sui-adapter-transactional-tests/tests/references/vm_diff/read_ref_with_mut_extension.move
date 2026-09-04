// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A read through a reference with a live mutable extension is allowed in a PTB, where Move rejects
// it with READREF_EXISTS_MUTABLE_BORROW_ERROR.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_val(): Inner { Inner { f: 0, g: 0 } }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun g_mut(i: &mut Inner): &mut u64 { &mut i.g }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun copy_inner(i: &Inner): Inner { *i }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun take_inner(_: Inner) {}
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 7
// VALID: the parent is read (frozen) while its `&mut u64` child is live, the child written after
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::copy_inner(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 7
// VALID: the parent is read by value while its `&mut u64` child is live, the child written after
//> 0: test::m::inner_val();
//> 1: test::m::id_mut<test::m::Inner>(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::take_inner(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
