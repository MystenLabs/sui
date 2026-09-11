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
public fun inner_val(): Inner { Inner { f: 0, g: 0 } }
public fun two_mut(_: &mut Inner, _: &mut Inner) { abort 0 }
public fun mut_imm(_: &mut Inner, _: &Inner) { abort 0 }
public fun imm_mut(_: &Inner, _: &mut Inner) { abort 0 }
public fun two_imm(_: &Inner, _: &Inner) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, mut twice
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::two_mut(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, mut then frozen
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::mut_imm(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, frozen then mut
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::imm_mut(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: a `&mut` result frozen twice in one call
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::two_imm(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: an immutable result twice in one call
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::two_imm(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 1, a value borrowed mutably and immutably in one call
//> 0: test::m::inner_val();
//> 1: test::m::mut_imm(Result(0), Result(0));
