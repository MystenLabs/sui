// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID }

public fun new(ctx: &mut TxContext): Obj { Obj { id: object::new(ctx) } }
public fun ctx_then_ref<T>(_: &mut T, r: &mut u64): &mut u64 { r }
public fun ref_then_ctx<T>(r: &mut u64, _: &mut T): &mut u64 { r }
public fun ref_then_imm_ctx<T>(r: &mut u64, _: &T): &mut u64 { r }
public fun two_mut_then_ref<T>(_: &mut T, _: &mut T, r: &mut u64): &mut u64 { r }
public fun mut_imm_then_ref<T>(_: &mut T, _: &T, r: &mut u64): &mut u64 { r }
public fun two_imm_then_ref<T>(_: &T, _: &T, r: &mut u64): &mut u64 { r }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun delete(o: Obj) { let Obj { id } = o; object::delete(id) }

//# programmable --sender A --inputs 1 7
// VALID: the generic `&mut T` slot is injected, the `&mut u64` survives a later `&mut TxContext` use
//> 0: test::m::ctx_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::new();
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));
//> 4: test::m::delete(Result(1));

//# programmable --sender A --inputs 1 7
// VALID: the injected generic slot last
//> 0: test::m::ref_then_ctx<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::new();
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));
//> 4: test::m::delete(Result(1));

//# programmable --sender A --inputs 1 7
// VALID: an immutable generic slot last
//> 0: test::m::ref_then_imm_ctx<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::new();
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));
//> 4: test::m::delete(Result(1));

//# programmable --sender A --inputs 1 7
// INVALID: two injected `&mut TxContext` through one generic, rejected like the non-generic case
//> 0: test::m::two_mut_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# programmable --sender A --inputs 1 7
// INVALID: injected `&mut TxContext` next to an injected `&TxContext` through one generic
//> 0: test::m::mut_imm_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# programmable --sender A --inputs 1 7
// VALID: two injected `&TxContext` through one generic, the user reference still usable
//> 0: test::m::two_imm_then_ref<sui::tx_context::TxContext>(Input(0));
//> 1: test::m::new();
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));
//> 4: test::m::delete(Result(1));
