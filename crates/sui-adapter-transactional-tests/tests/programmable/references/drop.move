// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A reference result that is never used is released at the end of the command that created it,
// so it never blocks a later command. Dropping the reference does not drop the values returned
// next to it.

//# init --addresses test=0x0 q=0x0 q_2=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Hot {}
public struct Potato { v: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun hot_and_ref(i: &mut Inner): (Hot, &mut u64) { (Hot {}, &mut i.f) }
public fun cool(h: Hot) { let Hot {} = h; }
public fun open(o: &mut Obj): (Potato, &mut Inner) { (Potato { v: 1 }, &mut o.inner) }
public fun close_with_parent(p: Potato, o: &mut Obj) { let Potato { v } = p; o.inner.g = v }
public fun id<T>(t: &T): &T { t }
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# programmable
// VALID: an unused `&mut Inner`, the parent used mutably right after
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: one of two sibling references unused, the parent of both then used
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_g_mut(Result(1));
//> 3: test::m::write(NestedResult(2,0), Input(0));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::delete(Result(0));

//# programmable
// VALID: an unused reference next to a hot potato; the reference is dropped, the potato is consumed
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::hot_and_ref(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::cool(NestedResult(2,0));
//> 5: test::m::delete(Result(0));

//# programmable --sender A
// VALID: the sibling reference unused and released, the potato closed against the parent
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_with_parent(NestedResult(1,0), Result(0));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: an unused reference at the end of the transaction
//> 0: test::m::new();
//> 1: test::m::delete(Result(0));
//> 2: sui::tx_context::digest();

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2)
// VALID: an unused reference into the ticket does not prevent Upgrade from consuming it
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: test::m::id<sui::package::UpgradeTicket>(Result(0));
//> 2: Upgrade(q_2, [sui, std], q, Result(0));
//> 3: sui::package::commit_upgrade(Input(0), Result(2));
