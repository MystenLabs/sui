// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reference arguments to a MoveCall that the transferability check rejects: a `&mut` with a live
// extension, a `&mut` aliased elsewhere in the same call, and an immutable reference extended by a
// `&mut` in the same call. The reported index is the first `&mut` parameter holding the offending
// reference, counting an injected TxContext.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Potato { v: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_val(): Inner { Inner { f: 0, g: 0 } }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun g(i: &Inner): &u64 { &i.g }
public fun g_mut(i: &mut Inner): &mut u64 { &mut i.g }
public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public struct Pair has copy, drop, store { left: vector<u64>, right: vector<u64> }
public fun pair(): Pair { Pair { left: vector[1], right: vector[2] } }
public fun split(p: &mut Pair): (&mut vector<u64>, &mut vector<u64>) { (&mut p.left, &mut p.right) }
public fun split_mixed(p: &mut Pair): (&mut vector<u64>, &vector<u64>) { (&mut p.left, &p.right) }
public fun first(v: &vector<u64>): &u64 { &v[0] }
public fun first_mut(v: &mut vector<u64>): &mut u64 { &mut v[0] }
public fun replace(v: &mut vector<u64>) { *v = vector[] }
public fun replace_pair(p: &mut Pair) { *p = pair() }
public fun uid_mut(o: &mut Obj): &mut UID { &mut o.id }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun boom(): &mut u64 { abort 7 }
public fun two_views<A, B>(_: &mut A, _: &mut B) {}
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun two_mut_inner(_: &mut Inner, _: &mut Inner) { abort 0 }
public fun mut_imm_inner(_: &mut Inner, _: &Inner) { abort 0 }
public fun imm_mut_inner(_: &Inner, _: &mut Inner) { abort 0 }
public fun inner_u64(_: &mut Inner, _: &mut u64) { abort 0 }
public fun u64_inner(_: &mut u64, _: &mut Inner) { abort 0 }
public fun inner_imm_u64(_: &mut Inner, _: &u64) { abort 0 }
public fun imm_inner_u64(_: &Inner, _: &mut u64) { abort 0 }
public fun inner_obj(_: &mut Inner, _: &mut Obj) { abort 0 }
public fun obj_inner(_: &mut Obj, _: &mut Inner) { abort 0 }
public fun u64_ctx_inner(_: &mut u64, _: &mut TxContext, _: &mut Inner) { abort 0 }
public fun inner_ctx_u64(_: &mut Inner, _: &mut TxContext, _: &mut u64) { abort 0 }
public fun imm_ctx_mut(_: &Inner, _: &mut TxContext, _: &mut Inner) { abort 0 }
public fun open(o: &mut Obj): (Potato, &mut Inner) { (Potato { v: 1 }, &mut o.inner) }
public fun close_with_parent(p: Potato, o: &mut Obj) { let Potato { v } = p; o.inner.g = v }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

// --- the same reference twice in one call ---

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, mut twice
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::two_mut_inner(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, mut then frozen
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::mut_imm_inner(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, frozen then mut
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::imm_mut_inner(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 1, a value borrowed mutably and immutably in one call
//> 0: test::m::inner_val();
//> 1: test::m::mut_imm_inner(Result(0), Result(0));

//# programmable --inputs 0u8
// INVALID: InvalidReferenceArgument at arg 0 of command 0, one pure input at one type twice
//> 0: test::m::two_views<u8, u8>(Input(0), Input(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 1, the same free-floating reference twice
//> 0: test::m::boom();
//> 1: test::m::two_mut(Result(0), Result(0));

// --- a parent and its child in one call ---

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
// INVALID: InvalidReferenceArgument at arg 0 of command 3, a `&mut` parent with its `&` child
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::g(Result(1));
//> 3: test::m::inner_imm_u64(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 3, a frozen parent with its `&mut` child; the child is the borrower
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::imm_inner_u64(Result(1), Result(2));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, a fresh `&mut` borrow of the root after its live child; the child is reported
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner_obj(Result(1), Result(0));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 2, a fresh `&mut` borrow of the root before its live child; the child is reported
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::obj_inner(Result(0), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --sender A
// INVALID: InvalidReferenceArgument at arg 1 of command 2, the parent `&mut` while the sibling reference from the same call is live
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_with_parent(NestedResult(1,0), Result(0));
//> 3: test::m::use_mut<test::m::Inner>(NestedResult(1,1));
//> 4: test::m::delete(Result(0));

// --- the reported index counts an injected TxContext ---

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

// --- a parent used mutably while a child is live in a later command ---

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, `&mut Obj` while the `&mut Inner` child is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 2, `&mut Obj` while the `&Inner` child is live
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::use_imm<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 3, a `&mut` result while its own child is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 3, `&mut Obj` while a grandchild is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_mut<test::m::Obj>(Result(0));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 3
// INVALID: InvalidReferenceArgument at arg 0 of command 4, the direct parent of a live alias
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::write(Result(3), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidReferenceArgument at arg 0 of command 1, `&mut Obj` on an owned object input while its child is live
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::use_mut<test::m::Obj>(Input(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidReferenceArgument at arg 0 of command 1, `&mut Obj` on an owned object input while an immutable child is live
//> 0: test::m::inner(Input(0));
//> 1: test::m::use_mut<test::m::Obj>(Input(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(0));

//# programmable --sender A
// INVALID: InvalidReferenceArgument at arg 0 of command 1, `&mut` of the gas coin while a reference into it is live
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

// --- descendants of sibling results ---

//# programmable --inputs 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, replace a `&mut` sibling while its own descendant is live
//> 0: test::m::pair();
//> 1: test::m::split(Result(0));
//> 2: test::m::first_mut(NestedResult(1,1));
//> 3: test::m::replace(NestedResult(1,1));
//> 4: test::m::check(Result(2), Input(0));

//# programmable --inputs 2
// INVALID: InvalidReferenceArgument at arg 0 of command 6, the same through a depth-three identity chain
//> 0: test::m::pair();
//> 1: test::m::split(Result(0));
//> 2: test::m::id_mut<vector<u64>>(NestedResult(1,1));
//> 3: test::m::id_mut<vector<u64>>(Result(2));
//> 4: test::m::id_mut<vector<u64>>(Result(3));
//> 5: test::m::first_mut(Result(4));
//> 6: test::m::replace(NestedResult(1,1));
//> 7: test::m::check(Result(5), Input(0));

//# programmable --inputs 2
// INVALID: InvalidReferenceArgument at arg 0 of command 3, replace the root while a descendant of the `&` sibling is live
//> 0: test::m::pair();
//> 1: test::m::split_mixed(Result(0));
//> 2: test::m::first(NestedResult(1,1));
//> 3: test::m::replace_pair(Result(0));
//> 4: test::m::check(Result(2), Input(0));

//# programmable --inputs 2
// INVALID: InvalidReferenceArgument at arg 0 of command 6, the same through a depth-three immutable identity chain
//> 0: test::m::pair();
//> 1: test::m::split_mixed(Result(0));
//> 2: test::m::id<vector<u64>>(NestedResult(1,1));
//> 3: test::m::id<vector<u64>>(Result(2));
//> 4: test::m::id<vector<u64>>(Result(3));
//> 5: test::m::first(Result(4));
//> 6: test::m::replace_pair(Result(0));
//> 7: test::m::check(Result(5), Input(0));

// --- dynamic fields ---

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 6, remove a dynamic field while a `&mut` into its stored vector is live
//> 0: test::m::new();
//> 1: test::m::uid_mut(Result(0));
//> 2: MakeMoveVec<u64>([Input(0)]);
//> 3: sui::dynamic_field::add<u64, vector<u64>>(Result(1), Input(0), Result(2));
//> 4: sui::dynamic_field::borrow_mut<u64, vector<u64>>(Result(1), Input(0));
//> 5: test::m::first_mut(Result(4));
//> 6: sui::dynamic_field::remove<u64, vector<u64>>(Result(1), Input(0));
//> 7: test::m::use_imm<u64>(Result(5));
//> 8: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: InvalidReferenceArgument at arg 0 of command 6, remove a dynamic field while a `&` into its stored vector is live
//> 0: test::m::new();
//> 1: test::m::uid_mut(Result(0));
//> 2: MakeMoveVec<u64>([Input(0)]);
//> 3: sui::dynamic_field::add<u64, vector<u64>>(Result(1), Input(0), Result(2));
//> 4: sui::dynamic_field::borrow<u64, vector<u64>>(Result(1), Input(0));
//> 5: test::m::first(Result(4));
//> 6: sui::dynamic_field::remove<u64, vector<u64>>(Result(1), Input(0));
//> 7: test::m::use_imm<u64>(Result(5));
//> 8: test::m::delete(Result(0));

// --- siblings from two calls conflict, dot-star is per call not per field ---

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

// --- immutable results of the parent block the mutable child ---

//# programmable
// INVALID: InvalidReferenceArgument at arg 0 of command 3, an immutable call on the parent returned a reference
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::inner(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::use_imm<test::m::Inner>(Result(2));
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

// --- vector natives ---

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, push_back while an element reference is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::push_back<u64>(Result(0), Input(3));
//> 3: test::m::write(Result(1), Input(3));

//# programmable --inputs 1 2 0 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, two mutable element borrows in two commands
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::borrow_mut<u64>(Result(0), Input(3));
//> 3: test::m::write(Result(1), Input(4));
//> 4: test::m::write(Result(2), Input(4));
