// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun freeze_new(ctx: &mut TxContext) {
    transfer::public_freeze_object(Obj { id: object::new(ctx), inner: Inner { f: 3, g: 0 } })
}
public fun inner(o: &Obj): &Inner { &o.inner }

public fun f(i: &Inner): &u64 { &i.f }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# programmable --sender A
//> 0: test::m::freeze_new();

//# programmable --sender A --inputs object(2,0) 3
// VALID: `&Obj` then `&Inner` then `&u64`
//> 0: test::m::inner(Input(0));
//> 1: test::m::f(Result(0));
//> 2: test::m::check(Result(1), Input(1));
