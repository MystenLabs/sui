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

public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs object(2,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, deleting while borrowed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::delete(Input(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(0));
