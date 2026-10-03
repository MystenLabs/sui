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
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable
// INVALID: `&mut Obj` while the `&mut Inner` child is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable
// INVALID: `&mut Obj` while the `&Inner` child is live
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_mut<test::m::Obj>(Result(0));
//> 3: test::m::use_imm<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: `&mut Inner` (itself a reference result) while its `&mut u64` child is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 1
// INVALID: `&mut Obj` while a grandchild is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_mut<test::m::Obj>(Result(0));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: `&mut Obj` on an owned object input while its child is live
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::use_mut<test::m::Obj>(Input(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(0));

//# programmable --sender A --inputs object(2,0)
// INVALID: `&mut Obj` on an owned object input while an immutable child is live
//> 0: test::m::inner(Input(0));
//> 1: test::m::use_mut<test::m::Obj>(Input(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(0));
