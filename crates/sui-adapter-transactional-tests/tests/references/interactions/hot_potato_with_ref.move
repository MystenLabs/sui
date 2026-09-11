// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Potato { v: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun open(o: &mut Obj): (Potato, &mut Inner) { (Potato { v: 1 }, &mut o.inner) }
public fun close(p: Potato, i: &mut Inner) { let Potato { v } = p; i.f = v }
public fun close_with_parent(p: Potato, o: &mut Obj) { let Potato { v } = p; o.inner.g = v }
public fun close_and_delete(p: Potato, o: Obj) { let Potato { v: _ } = p; delete(o) }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --inputs 1
// VALID: the potato and the reference consumed by one call, the write observed after
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close(NestedResult(1,0), NestedResult(1,1));
//> 3: test::m::inner(Result(0));
//> 4: test::m::f(Result(3));
//> 5: test::m::check(Result(4), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --sender A
// INVALID: InvalidReferenceArgument at arg 1 of command 2, the parent `&mut` while the sibling reference is live
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_with_parent(NestedResult(1,0), Result(0));
//> 3: test::m::use_mut<test::m::Inner>(NestedResult(1,1));
//> 4: test::m::delete(Result(0));

//# programmable --sender A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, the parent by value while the sibling reference is live
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_and_delete(NestedResult(1,0), Result(0));
//> 3: test::m::use_mut<test::m::Inner>(NestedResult(1,1));

//# programmable --sender A
// VALID: the parent consumed with the potato once the reference is dead
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::use_mut<test::m::Inner>(NestedResult(1,1));
//> 3: test::m::close_and_delete(NestedResult(1,0), Result(0));

//# programmable --sender A
// INVALID: UnusedValueWithoutDrop { result_idx: 1, secondary_idx: 0 }, the potato never consumed
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::use_mut<test::m::Inner>(NestedResult(1,1));
//> 3: test::m::delete(Result(0));

//# programmable --sender A
// VALID: the reference unused and released, the potato closed against the parent
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_with_parent(NestedResult(1,0), Result(0));
//> 3: test::m::delete(Result(0));
