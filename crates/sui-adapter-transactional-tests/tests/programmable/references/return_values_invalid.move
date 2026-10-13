// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A returned reference extends every mutable argument of its call (every argument, when
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
public fun pick_imm_from_imm(_a: &mut Inner, b: &Inner): &u64 { &b.f }
public fun pick_imm_from_mut(a: &mut Inner, _b: &Inner): &u64 { &a.f }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun f(i: &Inner): &u64 { &i.f }
public fun imm_val_mut(a: &vector<u64>, b: &mut vector<u64>): (&vector<u64>, u64, &mut vector<u64>) { (a, 1, b) }
public fun replace(v: &mut vector<u64>) { *v = vector[] }
public fun digest_via(ctx: &TxContext, _x: &u64): &vector<u8> { ctx.digest() }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 5, the source the callee did not return is blocked too
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

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 5, an immutable result of `(&mut A, &B)` returned from B still blocks a `&mut` of A
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick_imm_from_imm(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(2));
//> 6: test::m::use_imm<u64>(Result(4));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 5, an immutable result of `(&mut A, &B)` returned from B blocks a `&mut` of B
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick_imm_from_imm(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(3));
//> 6: test::m::use_imm<u64>(Result(4));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 5, an immutable result of `(&mut A, &B)` returned from A still blocks a `&mut` of B
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick_imm_from_mut(Result(2), Result(3));
//> 5: test::m::use_mut<test::m::Inner>(Result(3));
//> 6: test::m::use_imm<u64>(Result(4));
//> 7: test::m::delete(Result(0));
//> 8: test::m::delete(Result(1));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 7, a chained `&mut` descendant of a two-source result blocks the returned source
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::id_mut<u64>(Result(4));
//> 6: test::m::id_mut<u64>(Result(5));
//> 7: test::m::use_mut<test::m::Inner>(Result(2));
//> 8: test::m::write(Result(6), Input(0));
//> 9: test::m::delete(Result(0));
//> 10: test::m::delete(Result(1));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 7, a chained `&mut` descendant of a two-source result blocks the source the callee did not return
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::id_mut<u64>(Result(4));
//> 6: test::m::id_mut<u64>(Result(5));
//> 7: test::m::use_mut<test::m::Inner>(Result(3));
//> 8: test::m::write(Result(6), Input(0));
//> 9: test::m::delete(Result(0));
//> 10: test::m::delete(Result(1));

//# programmable --inputs 1 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, with the `&mut U` result unused, the live `&T` result of `(&T, value, &mut U)` still extends the mutable source U
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: MakeMoveVec<u64>([Input(1)]);
//> 2: test::m::imm_val_mut(Result(0), Result(1));
//> 3: test::m::replace(Result(1));
//> 4: test::m::use_imm<vector<u64>>(NestedResult(2,0));

//# programmable --inputs 1 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, the live `&T` result also extends its own source T
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: MakeMoveVec<u64>([Input(1)]);
//> 2: test::m::imm_val_mut(Result(0), Result(1));
//> 3: test::m::replace(Result(0));
//> 4: test::m::use_imm<vector<u64>>(NestedResult(2,0));

//# programmable --inputs 1 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, with the `&T` result unused, the live `&mut U` result blocks a `&mut` of U
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: MakeMoveVec<u64>([Input(1)]);
//> 2: test::m::imm_val_mut(Result(0), Result(1));
//> 3: test::m::replace(Result(1));
//> 4: test::m::use_mut<vector<u64>>(NestedResult(2,2));

//# programmable --inputs 0
// INVALID: InvalidReferenceArgument at arg 0 of command 1, a laundered TxContext reference blocks a `&mut` of the other argument
//> 0: test::m::digest_via(Input(0));
//> 1: test::m::use_mut<u64>(Input(0));
//> 2: test::m::use_imm<vector<u8>>(Result(0));
