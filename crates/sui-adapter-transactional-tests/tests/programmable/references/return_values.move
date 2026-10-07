// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// How a reference returned by a MoveCall relates to the call's arguments: a mutable result extends
// every mutable argument, an immutable result extends every argument, TxContext is never a source,
// and the root may be any location kind.

//# init --addresses test=0x0 q=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun inner_frozen(o: &mut Obj): &Inner { &o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun f_mut_g(i: &mut Inner): (&mut u64, &u64) { (&mut i.f, &i.g) }
public fun f_g(i: &Inner): (&u64, &u64) { (&i.f, &i.g) }
public fun val_and_ref(i: &mut Inner): (u64, &mut u64) { (i.g, &mut i.f) }
public fun pick(a: &mut Inner, _b: &mut Inner): &mut u64 { &mut a.f }
public fun pick_mixed(a: &mut Inner, _b: &Inner): &mut u64 { &mut a.f }
public fun imm_val_mut(a: &vector<u64>, b: &mut vector<u64>): (&vector<u64>, u64, &mut vector<u64>) { (a, 1, b) }
public fun first(v: &vector<u64>): &u64 { &v[0] }
public fun replace(v: &mut vector<u64>) { *v = vector[] }
public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun first_mut<T>(v: &mut vector<T>): &mut T { &mut v[0] }
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun mut_imm(_: &mut u64, _: &u64) {}
public fun two_imm(_: &u64, _: &u64) {}
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun use_val(_: u64) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_val(x: u64, v: u64) { assert!(x == v, 0) }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun check_id(id: &ID, o: &Obj) { assert!(*id == object::id(o), 0) }
public fun digest_via(ctx: &TxContext, _x: &u64): &vector<u8> { ctx.digest() }
public fun fresh(ctx: &mut TxContext): address { ctx.fresh_object_address() }
public fun eq_bytes(a: &vector<u8>, b: &vector<u8>) { assert!(a == b, 0) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# stage-package
module q::m {
    public fun x(): u64 { 0 }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

// --- which arguments a result extends ---

//# programmable --inputs 1
// VALID: both sources of a two-source `&mut` result are usable once the result is dead
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

//# programmable
// VALID: `&Inner` from `&mut Obj`
//> 0: test::m::new();
//> 1: test::m::inner_frozen(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: `&Inner` from `&Obj`
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: both sources usable again once a chained descendant of a two-source result is dead
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::inner_mut(Result(0));
//> 3: test::m::inner_mut(Result(1));
//> 4: test::m::pick(Result(2), Result(3));
//> 5: test::m::id_mut<u64>(Result(4));
//> 6: test::m::id_mut<u64>(Result(5));
//> 7: test::m::write(Result(6), Input(0));
//> 8: test::m::use_mut<test::m::Inner>(Result(2));
//> 9: test::m::use_mut<test::m::Inner>(Result(3));
//> 10: test::m::delete(Result(0));
//> 11: test::m::delete(Result(1));

// --- multiple results from one call ---

//# programmable --inputs 1 2 9 1 0
// VALID: (&T, value, &mut U) keeps result indexes and mutability
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: MakeMoveVec<u64>([Input(1)]);
//> 2: test::m::imm_val_mut(Result(0), Result(1));
//> 3: test::m::first_mut<u64>(NestedResult(2,2));
//> 4: test::m::write(Result(3), Input(2));
//> 5: test::m::first(NestedResult(2,0));
//> 6: test::m::check(Result(5), Input(0));
//> 7: test::m::check_val(NestedResult(2,1), Input(3));
//> 8: test::m::check_elem(Result(1), Input(4), Input(2));

//# programmable --inputs 1 2 9 0
// VALID: with the `&T` result unused, the `&mut U` result extends only its mutable source, so the other root is free
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: MakeMoveVec<u64>([Input(1)]);
//> 2: test::m::imm_val_mut(Result(0), Result(1));
//> 3: test::m::replace(Result(0));
//> 4: test::m::first_mut<u64>(NestedResult(2,2));
//> 5: test::m::write(Result(4), Input(2));
//> 6: test::m::check_elem(Result(1), Input(3), Input(2));


//# programmable --inputs 1 2
// VALID: (mut, mut) written separately and passed together
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::write(NestedResult(2,1), Input(1));
//> 5: test::m::two_mut(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 1 0
// VALID: (mut, imm) written and read while both are live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut_g(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::check(NestedResult(2,1), Input(1));
//> 5: test::m::mut_imm(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 0
// VALID: (imm, imm)
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g(Result(1));
//> 3: test::m::check(NestedResult(2,0), Input(0));
//> 4: test::m::check(NestedResult(2,1), Input(0));
//> 5: test::m::two_imm(NestedResult(2,0), NestedResult(2,1));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 3
// VALID: (value, mut ref)
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::val_and_ref(Result(1));
//> 3: test::m::use_val(NestedResult(2,0));
//> 4: test::m::write(NestedResult(2,1), Input(0));
//> 5: test::m::delete(Result(0));

// --- generic callees ---

//# programmable --inputs 4
// VALID: generic identity on an object, an inner struct, and a field
//> 0: test::m::new();
//> 1: test::m::id_mut<test::m::Obj>(Result(0));
//> 2: test::m::inner_mut(Result(1));
//> 3: test::m::id_mut<test::m::Inner>(Result(2));
//> 4: test::m::f_mut(Result(3));
//> 5: test::m::id_mut<u64>(Result(4));
//> 6: test::m::write(Result(5), Input(0));
//> 7: test::m::check(Result(4), Input(0));
//> 8: test::m::delete(Result(0));

//# programmable --inputs 1 0 9
// VALID: generic element borrow of a vector result
//> 0: MakeMoveVec<u64>([Input(0)]);
//> 1: test::m::first_mut<u64>(Result(0));
//> 2: test::m::write(Result(1), Input(2));
//> 3: test::m::check_elem(Result(0), Input(1), Input(2));

//# programmable --inputs 5
// VALID: generic immutable identity on a pure input
//> 0: test::m::id<u64>(Input(0));
//> 1: test::m::check(Result(0), Input(0));

// --- roots other than MoveCall results ---

//# programmable --sender A --inputs 100 @A
// VALID: a reference rooted in a SplitCoins result
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: sui::coin::value<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(0)], Input(1));

//# programmable --inputs 1 2 0 9
// VALID: a reference rooted in a MakeMoveVec result
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: test::m::write(Result(1), Input(3));
//> 3: test::m::check_elem(Result(0), Input(2), Input(3));

//# programmable --sender A --inputs @A
// VALID: a reference rooted in a Publish result (the UpgradeCap)
//> 0: Publish(q, []);
//> 1: test::m::id_mut<sui::package::UpgradeCap>(Result(0));
//> 2: sui::package::version(Result(1));
//> 3: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(3,0)
// VALID: `&ID` from a framework borrow of an object input, then the object read again
//> 0: sui::object::borrow_id<test::m::Obj>(Input(0));
//> 1: sui::object::id_to_address(Result(0));
//> 2: test::m::check_id(Result(0), Input(0));

// --- TxContext is never a source ---

//# programmable
// VALID: a reference into TxContext survives later mutable TxContext uses and stays consistent
//> 0: sui::tx_context::digest();
//> 1: test::m::fresh();
//> 2: test::m::fresh();
//> 3: sui::tx_context::digest();
//> 4: test::m::eq_bytes(Result(0), Result(3));

//# programmable --inputs 0
// VALID: a laundered TxContext reference is attributed to the other argument, immutable uses are fine
//> 0: test::m::digest_via(Input(0));
//> 1: test::m::use_imm<u64>(Input(0));
//> 2: test::m::fresh();
//> 3: test::m::use_imm<vector<u8>>(Result(0));
