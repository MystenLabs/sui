// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A returned reference extends every mutable argument of its call (all arguments, when
// immutable), regardless of which one the callee actually returned.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun pick(a: &mut Inner, _b: &mut Inner): &mut u64 { &mut a.f }
public fun pick_imm(a: &Inner, _b: &Inner): &u64 { &a.f }
public fun pick_mixed(a: &mut Inner, _b: &Inner): &mut u64 { &mut a.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 5, the second source is blocked too
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(3));
//> 6: test::m::write(Result(4), Input(0));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 5, the returned source is blocked
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(2));
//> 6: test::m::write(Result(4), Input(0));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable --inputs 1
// VALID: both sources usable once the result is dead
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::write(Result(4), Input(0));
//> 6: test::m::use_mut<test::m::Inner>(Result(2));
//> 7: test::m::use_mut<test::m::Inner>(Result(3));
//> 8: test::m::delete(Result(0));
//> 9: test::m::delete(Result(1));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 5, an immutable result blocks a `&mut` of an immutable source
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick_imm(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(3));
//> 6: test::m::use_imm<u64>(Result(4));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable --inputs 1
// VALID: a mutable result of a mixed call does not extend the immutable argument
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick_mixed(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(3));
//> 6: test::m::write(Result(4), Input(0));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));
