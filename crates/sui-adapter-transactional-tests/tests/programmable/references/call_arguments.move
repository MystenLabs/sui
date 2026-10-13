// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reference arguments to a MoveCall that the transferability check accepts: aliases of immutable
// references, siblings from one call, parents reused after their children die, and immutable
// views of a parent while a mutable child is live.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Potato { v: u64 }
public struct NoCopy has drop { value: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun g_mut(i: &mut Inner): &mut u64 { &mut i.g }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun f_mut_g(i: &mut Inner): (&mut u64, &u64) { (&mut i.f, &i.g) }
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
public fun two_imm_inner(_: &Inner, _: &Inner) {}
public fun two_mut(a: &mut u64, b: &mut u64) { *a = *a + 1; *b = *b + 1 }
public fun mut_imm(a: &mut u64, b: &u64) { *a = *b }
public fun two_views<A, B>(_: &mut A, _: &mut B) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun write_u8(r: &mut u8, v: u8) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_len(v: &vector<u64>, n: u64) { assert!(v.length() == n, 0) }
public fun open(o: &mut Obj): (Potato, &mut Inner) { (Potato { v: 1 }, &mut o.inner) }
public fun close(p: Potato, i: &mut Inner) { let Potato { v } = p; i.f = v }
public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun read_then_mut(x: u64, n: &mut NoCopy) { n.value = x + 1 }
public fun mut_then_read(n: &mut NoCopy, x: u64) { n.value = x + 1 }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

// --- the same reference twice in one call ---

//# programmable
// VALID: a `&mut` result frozen twice in one call
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::two_imm_inner(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: an immutable result twice in one call
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::two_imm_inner(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 0u8
// VALID: one pure input at two types is two locations, so both `&mut` borrows are fine
//> 0: test::m::two_views<u8, bool>(Input(0), Input(0));

//# programmable --inputs 0u8 7u8
// VALID: a `&mut bool` view live while the u8 view is borrowed mutably
//> 0: test::m::id_mut<bool>(Input(0));
//> 1: test::m::id_mut<u8>(Input(0));
//> 2: test::m::write_u8(Result(1), Input(1));
//> 3: test::m::use_mut<bool>(Result(0));

// --- siblings ---

//# programmable --inputs 1
// VALID: two `&mut` siblings from one call passed together, both written
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::two_mut(NestedResult(2,0), NestedResult(2,1));
//> 4: test::m::check(NestedResult(2,0), Input(0));
//> 5: test::m::check(NestedResult(2,1), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 0
// VALID: a `&mut` and a `&` sibling from one call passed together
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut_g(Result(1));
//> 3: test::m::mut_imm(NestedResult(2,0), NestedResult(2,1));
//> 4: test::m::check(NestedResult(2,0), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: `&mut` references into two different objects passed together
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::f_mut(Result(2));
//> 5: test::m::f_mut(Result(3));
//> 6: test::m::two_mut(Result(4), Result(5));
//> 7: test::m::check(Result(4), Input(0));
//> 8: test::m::delete(Result(0));
//> 9: test::m::delete(Result(1));

//# programmable --inputs 1 2
// VALID: a second field borrow from the same parent once the first is dead
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::g_mut(Result(1));
//> 5: test::m::write(Result(4), Input(1));
//> 6: test::m::delete(Result(0));

//# programmable --sender A --inputs 1
// VALID: a hot potato and a reference from one call consumed together, the write observed after
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close(NestedResult(1,0), NestedResult(1,1));
//> 3: test::m::inner(Result(0));
//> 4: test::m::f(Result(3));
//> 5: test::m::check(Result(4), Input(0));
//> 6: test::m::delete(Result(0));

// --- descendants of sibling results ---

//# programmable --inputs 2
// VALID: replace one `&mut` sibling while a descendant of the other is live
//> 0: test::m::pair();
//> 1: test::m::split(Result(0));
//> 2: test::m::first_mut(NestedResult(1,1));
//> 3: test::m::replace(NestedResult(1,0));
//> 4: test::m::check(Result(2), Input(0));

//# programmable --inputs 2
// VALID: the same through a depth-three identity chain
//> 0: test::m::pair();
//> 1: test::m::split(Result(0));
//> 2: test::m::id_mut<vector<u64>>(NestedResult(1,1));
//> 3: test::m::id_mut<vector<u64>>(Result(2));
//> 4: test::m::id_mut<vector<u64>>(Result(3));
//> 5: test::m::first_mut(Result(4));
//> 6: test::m::replace(NestedResult(1,0));
//> 7: test::m::check(Result(5), Input(0));

//# programmable --inputs 0
// VALID: replace a sibling once its own descendant is dead
//> 0: test::m::pair();
//> 1: test::m::split(Result(0));
//> 2: test::m::first_mut(NestedResult(1,1));
//> 3: test::m::use_imm<u64>(Result(2));
//> 4: test::m::replace(NestedResult(1,1));
//> 5: test::m::check_len(NestedResult(1,1), Input(0));

//# programmable --inputs 2
// VALID: replace the `&mut` sibling while a descendant of the `&` sibling is live
//> 0: test::m::pair();
//> 1: test::m::split_mixed(Result(0));
//> 2: test::m::id<vector<u64>>(NestedResult(1,1));
//> 3: test::m::first(Result(2));
//> 4: test::m::replace(NestedResult(1,0));
//> 5: test::m::check(Result(3), Input(0));

//# programmable
// VALID: replace the root once the `&` sibling's descendant is dead
//> 0: test::m::pair();
//> 1: test::m::split_mixed(Result(0));
//> 2: test::m::first(NestedResult(1,1));
//> 3: test::m::use_imm<u64>(Result(2));
//> 4: test::m::replace_pair(Result(0));

// --- dynamic fields ---

//# programmable --inputs 1
// VALID: remove a dynamic field once the `&mut` into its stored vector is dead
//> 0: test::m::new();
//> 1: test::m::uid_mut(Result(0));
//> 2: MakeMoveVec<u64>([Input(0)]);
//> 3: sui::dynamic_field::add<u64, vector<u64>>(Result(1), Input(0), Result(2));
//> 4: sui::dynamic_field::borrow_mut<u64, vector<u64>>(Result(1), Input(0));
//> 5: test::m::first_mut(Result(4));
//> 6: test::m::use_imm<u64>(Result(5));
//> 7: sui::dynamic_field::remove<u64, vector<u64>>(Result(1), Input(0));
//> 8: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: remove a dynamic field once the `&` into its stored vector is dead
//> 0: test::m::new();
//> 1: test::m::uid_mut(Result(0));
//> 2: MakeMoveVec<u64>([Input(0)]);
//> 3: sui::dynamic_field::add<u64, vector<u64>>(Result(1), Input(0), Result(2));
//> 4: sui::dynamic_field::borrow<u64, vector<u64>>(Result(1), Input(0));
//> 5: test::m::first(Result(4));
//> 6: test::m::use_imm<u64>(Result(5));
//> 7: sui::dynamic_field::remove<u64, vector<u64>>(Result(1), Input(0));
//> 8: test::m::delete(Result(0));

// --- parents after children die ---

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

//# programmable --inputs 3
// VALID: write at the leaf of a chain, then reopen each level upward
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::write(Result(3), Input(0));
//> 5: test::m::check(Result(2), Input(0));
//> 6: test::m::use_mut<test::m::Inner>(Result(1));
//> 7: test::m::use_mut<test::m::Obj>(Result(0));
//> 8: test::m::delete(Result(0));

// --- immutable views of a parent while a mutable child is live ---

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
// VALID: an immutable result of the parent is readable while the mutable child is dormant
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

// --- a last-use read releases its reference inside the argument list ---

//# programmable --inputs 1
// VALID: read then a fresh `&mut` borrow of the root
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::read_then_mut(Result(1), Result(0));

//# programmable --inputs 1
// VALID: a fresh `&mut` borrow of the root then the read; transferability is checked after all arguments
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::mut_then_read(Result(0), Result(1));

// --- vector natives ---

//# programmable --inputs 1 2 0 9 3
// VALID: length while a `&mut` element is live; push after the element reference is dead
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::length<u64>(Result(0));
//> 3: test::m::write(Result(1), Input(3));
//> 4: std::vector::push_back<u64>(Result(0), Input(3));
//> 5: test::m::check_len(Result(0), Input(4));

//# programmable --inputs 1 2 0 1
// VALID: two immutable element borrows in two commands
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow<u64>(Result(0), Input(2));
//> 2: std::vector::borrow<u64>(Result(0), Input(3));
//> 3: test::m::check(Result(1), Input(0));
//> 4: test::m::check(Result(2), Input(1));
