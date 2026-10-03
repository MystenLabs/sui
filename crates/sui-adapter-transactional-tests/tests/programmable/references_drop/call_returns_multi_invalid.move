// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Hot {}

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }

public fun hot_and_ref(i: &mut Inner): (Hot, &mut u64) { (Hot {}, &mut i.f) }

public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// INVALID: UnusedValueWithoutDrop for result (2, 0), the hot potato next to a dropped reference
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::hot_and_ref(Result(1));
//> 3: test::m::delete(Result(0));
