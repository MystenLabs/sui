// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Parent has key, store { id: UID }
public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new_parent(ctx: &mut TxContext): Parent { Parent { id: object::new(ctx) } }
public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun uid(p: &Parent): &UID { &p.id }
public fun uid_mut(p: &mut Parent): &mut UID { &mut p.id }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun fail() { abort 42 }

//# programmable --sender A --inputs @A
//> 0: test::m::new_parent();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 1
// VALID: a fresh object added as a dynamic object field through the `&mut UID` reference
//> 0: test::m::uid_mut(Input(0));
//> 1: test::m::new();
//> 2: sui::dynamic_object_field::add<u64, test::m::Obj>(Result(0), Input(1), Result(1));

//# programmable --sender A --inputs object(2,0) 1 9
// VALID: a write through the child object chain
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_object_field::borrow_mut<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner_mut(Result(1));
//> 3: test::m::f_mut(Result(2));
//> 4: test::m::write(Result(3), Input(2));

//# programmable --sender A --inputs object(2,0) 1 9
// VALID: the write is visible through an immutable chain
//> 0: test::m::uid(Input(0));
//> 1: sui::dynamic_object_field::borrow<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner(Result(1));
//> 3: test::m::f(Result(2));
//> 4: test::m::check(Result(3), Input(2));

//# programmable --sender A --inputs object(2,0) 1 5
// INVALID: abort 42 at command 5 after a write through the child
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_object_field::borrow_mut<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner_mut(Result(1));
//> 3: test::m::f_mut(Result(2));
//> 4: test::m::write(Result(3), Input(2));
//> 5: test::m::fail();

//# programmable --sender A --inputs object(2,0) 1 9
// VALID: the write from the failed transaction was discarded
//> 0: test::m::uid(Input(0));
//> 1: sui::dynamic_object_field::borrow<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner(Result(1));
//> 3: test::m::f(Result(2));
//> 4: test::m::check(Result(3), Input(2));

//# programmable --sender A --inputs object(2,0) 1 @A
// INVALID: InvalidReferenceArgument at arg 0 of command 3, remove while the child chain is live
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_object_field::borrow_mut<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner_mut(Result(1));
//> 3: sui::dynamic_object_field::remove<u64, test::m::Obj>(Result(0), Input(1));
//> 4: test::m::use_mut<test::m::Inner>(Result(2));
//> 5: TransferObjects([Result(3)], Input(2));

//# programmable --sender A --inputs object(2,0) 1 @A
// VALID: removed and transferred once the chain is dead
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_object_field::borrow_mut<u64, test::m::Obj>(Result(0), Input(1));
//> 2: test::m::inner_mut(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(2));
//> 4: sui::dynamic_object_field::remove<u64, test::m::Obj>(Result(0), Input(1));
//> 5: TransferObjects([Result(4)], Input(2));
