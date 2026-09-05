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
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A
//> 0: test::m::freeze_new();

//# programmable --sender A --inputs object(2,0) 3
// VALID: `&Obj` then `&Inner` then `&u64`
//> 0: test::m::inner(Input(0));
//> 1: test::m::f(Result(0));
//> 2: test::m::check(Result(1), Input(1));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidObjectByMutRef at arg 0 of command 0
//> 0: test::m::inner_mut(Input(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: TypeMismatch at arg 0 of command 1, `&Inner` used as `&mut Inner`
//> 0: test::m::inner(Input(0));
//> 1: test::m::use_mut<test::m::Inner>(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidObjectByValue at arg 0 of command 0
//> 0: test::m::delete(Input(0));
