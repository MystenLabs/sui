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
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun take_inner(i: Inner, f: u64) { assert!(i.f == f, 0) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 5
// VALID: a frozen read of the parent after a write through the child
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::check_inner(Result(1), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 5
// VALID: an immutable child taken after the write sees it
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::f(Result(1));
//> 5: test::m::check(Result(4), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 5 6
// VALID: two writes through two successive references, the last wins
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::f_mut(Result(1));
//> 5: test::m::write(Result(4), Input(1));
//> 6: test::m::check_inner(Result(1), Input(1));
//> 7: test::m::delete(Result(0));
