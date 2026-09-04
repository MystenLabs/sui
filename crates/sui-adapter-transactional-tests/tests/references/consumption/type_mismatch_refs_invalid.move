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
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun take_obj(o: Obj) { delete(o) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// INVALID: TypeMismatch at arg 0 of command 2, `&Inner` as `&mut Inner`
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// INVALID: TypeMismatch at arg 0 of command 2, `&Obj` into a by-value `Obj` parameter (no copy)
//> 0: test::m::new();
//> 1: test::m::id_mut<test::m::Obj>(Result(0));
//> 2: test::m::take_obj(Result(1));

//# programmable
// INVALID: TypeMismatch at arg 0 of command 2, `&mut Inner` where `&mut u64` is expected
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::id_mut<u64>(Result(1));
//> 3: test::m::delete(Result(0));
