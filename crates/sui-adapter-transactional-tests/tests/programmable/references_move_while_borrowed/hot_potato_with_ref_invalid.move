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

public fun close_and_delete(p: Potato, o: Obj) { let Potato { v: _ } = p; delete(o) }

public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, the parent by value while the sibling reference is live
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_and_delete(NestedResult(1,0), Result(0));
//> 3: test::m::use_mut<test::m::Inner>(NestedResult(1,1));
