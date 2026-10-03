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
public fun g(i: &Inner): &u64 { &i.g }
public fun mut_imm(_: &mut Inner, _: &u64) { abort 0 }
public fun imm_mut(_: &Inner, _: &mut u64) { abort 0 }
public fun inner_obj(_: &mut Inner, _: &mut Obj) { abort 0 }
public fun obj_inner(_: &mut Obj, _: &mut Inner) { abort 0 }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 3, a `&mut` parent with its `&` child in one call
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::g(Result(1));
//> 3: test::m::mut_imm(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 3, a frozen parent with its `&mut` child; the child is the borrower
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::imm_mut(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, the child precedes a fresh mutable borrow of its root
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner_obj(Result(1), Result(0));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, the child follows a fresh mutable borrow of its root
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::obj_inner(Result(0), Result(1));
//> 3: test::m::delete(Result(0));
