// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The reported argument index of InvalidReferenceArgument is the first `&mut` parameter holding
// the extended reference, counting an injected TxContext parameter.

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
public fun inner_u64(_: &mut Inner, _: &mut u64) { abort 0 }
public fun u64_inner(_: &mut u64, _: &mut Inner) { abort 0 }
public fun u64_ctx_inner(_: &mut u64, _: &mut TxContext, _: &mut Inner) { abort 0 }
public fun inner_ctx_u64(_: &mut Inner, _: &mut TxContext, _: &mut u64) { abort 0 }
public fun imm_ctx_mut(_: &Inner, _: &mut TxContext, _: &mut Inner) { abort 0 }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 3, parent first
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::inner_u64(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 3, parent second
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::u64_inner(Result(2), Result(1));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 2 of command 3, parent after an injected TxContext
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::u64_ctx_inner(Result(2), Result(1));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 3, parent before an injected TxContext
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::inner_ctx_u64(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 2 of command 2, the `&mut` alias after an injected TxContext
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::imm_ctx_mut(Result(1), Result(1));
//> 3: test::m::delete(Result(0));
