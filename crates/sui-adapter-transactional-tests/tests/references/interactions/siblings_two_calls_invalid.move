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
public fun g_mut(i: &mut Inner): &mut u64 { &mut i.g }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 1 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, disjoint fields still conflict across calls
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::g_mut(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::write(Result(3), Input(1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 3, an immutable reference blocks a later `&mut` borrow
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f(Result(1));
//> 3: test::m::f_mut(Result(1));
//> 4: test::m::write(Result(3), Input(0));
//> 5: test::m::use_imm<u64>(Result(2));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 1 2
// VALID: the second borrow is fine once the first reference is dead
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::g_mut(Result(1));
//> 5: test::m::write(Result(4), Input(1));
//> 6: test::m::delete(Result(0));
