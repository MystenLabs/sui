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
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun copy_inner(i: &Inner): Inner { *i }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 7 0
// VALID: freezing the parent permits a read while its mutable child remains writable, and the copy retains the old value
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::copy_inner(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
//> 6: test::m::check_inner(Result(3), Input(1));
//> 7: test::m::delete(Result(0));
