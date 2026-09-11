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
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --inputs 3
// VALID: write at the leaf, then reopen each level upward
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::write(Result(3), Input(0));
//> 5: test::m::check(Result(2), Input(0));
//> 6: test::m::use_mut<test::m::Inner>(Result(1));
//> 7: test::m::use_mut<test::m::Obj>(Result(0));
//> 8: test::m::delete(Result(0));

//# programmable --inputs 3
// INVALID: InvalidReferenceArgument at arg 0 of command 4, the middle of the chain while the leaf is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::write(Result(3), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 3
// INVALID: InvalidReferenceArgument at arg 0 of command 4, the root while the leaf is live
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::use_mut<test::m::Obj>(Result(0));
//> 5: test::m::write(Result(3), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 3
// INVALID: InvalidReferenceArgument at arg 0 of command 4, the direct parent of a live alias
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::id_mut<u64>(Result(2));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::write(Result(3), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 3
// VALID: identity chains of depth three on a pure input
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::id_mut<u64>(Result(0));
//> 2: test::m::id_mut<u64>(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::check(Result(1), Input(0));
//> 5: test::m::check(Result(0), Input(0));
//> 6: test::m::check(Input(0), Input(0));
