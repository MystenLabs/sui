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
public fun take_then_mut(o: Obj, _: &mut Inner) { delete(o) }

public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0)
// VALID: the object input by value with a reference into a different object
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::take_then_mut(Input(0), Result(1));
//> 3: test::m::delete(Result(0));
