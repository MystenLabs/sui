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
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }

public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A
//> 0: test::m::freeze_new();

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidObjectByMutRef at arg 0 of command 0
//> 0: test::m::inner_mut(Input(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: InvalidObjectByValue at arg 0 of command 0
//> 0: test::m::delete(Input(0));
