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
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun hot_and_ref(i: &mut Inner): (Hot, &mut u64) { (Hot {}, &mut i.f) }
public fun cool(h: Hot) { let Hot {} = h; }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// VALID: unused `&mut Inner`, parent used mutably right after
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: one of two references unused, the parent of both written through the other
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::delete(Result(0));

//# programmable
// VALID: an unused reference next to a consumed hot potato
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::hot_and_ref(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::cool(NestedResult(2,0));
//> 5: test::m::delete(Result(0));

//# programmable
// VALID: an unused reference at the end of the transaction
//> 0: test::m::new();
//> 1: test::m::delete(Result(0));
//> 2: sui::tx_context::digest();
