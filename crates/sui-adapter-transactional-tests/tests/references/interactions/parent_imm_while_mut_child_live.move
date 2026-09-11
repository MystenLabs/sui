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
public fun g(i: &Inner): &u64 { &i.g }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// VALID: `&Obj` while the `&mut Inner` child is live, child still writable after
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_imm<test::m::Obj>(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: a frozen use of the `&mut Inner` while its `&mut u64` child is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_imm<test::m::Inner>(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 3, the immutable call returned a reference
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::use_imm<test::m::Inner>(Result(2));
//> 5: test::m::delete(Result(0));

//# programmable
// VALID: the immutable result is readable while the mutable child is dormant
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner(Result(0));
//> 3: test::m::use_imm<test::m::Inner>(Result(2));
//> 4: test::m::use_imm<test::m::Inner>(Result(2));
//> 5: test::m::use_imm<test::m::Inner>(Result(1));
//> 6: test::m::delete(Result(0));

//# programmable
// VALID: the mutable child is usable again once the immutable result is dead
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner(Result(0));
//> 3: test::m::use_imm<test::m::Inner>(Result(2));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 4, an immutable reference from a frozen use blocks the sibling `&mut`
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::g(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::use_imm<u64>(Result(3));
//> 6: test::m::delete(Result(0));
