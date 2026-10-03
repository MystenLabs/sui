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
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun f(i: &Inner): &u64 { &i.f }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// VALID: `&mut Obj` after the `&mut Inner` child's last use
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(1));
//> 3: test::m::use_mut<test::m::Obj>(Result(0));
//> 4: test::m::delete(Result(0));

//# programmable
// VALID: `&mut Obj` after the `&Inner` child's last use
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::use_mut<test::m::Obj>(Result(0));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 5
// VALID: a chain released leaf first, each level usable after
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::use_mut<test::m::Obj>(Result(0));
//> 6: test::m::inner(Result(0));
//> 7: test::m::f(Result(6));
//> 8: test::m::check(Result(7), Input(0));
//> 9: test::m::delete(Result(0));
