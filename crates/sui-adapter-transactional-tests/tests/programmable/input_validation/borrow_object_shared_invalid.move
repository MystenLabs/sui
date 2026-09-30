// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } })
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs immshared(2,0)
// INVALID: InvalidObjectByMutRef at arg 0 of command 0
//> 0: test::m::inner_mut(Input(0));
