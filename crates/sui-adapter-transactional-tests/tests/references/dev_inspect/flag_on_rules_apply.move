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
public fun two_mut(_: &mut Inner, _: &mut Inner) { abort 0 }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable --sender A --dev-inspect --inputs 5
// VALID: a chain of references written through
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::delete(Result(0));

//# programmable --sender A --dev-inspect
// INVALID: InvalidReferenceArgument at arg 1 of command 2
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::two_mut(Result(1), Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --sender A --dev-inspect
// INVALID: InvalidReferenceArgument at arg 0 of command 3
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
//> 4: test::m::use_mut<u64>(Result(2));
//> 5: test::m::delete(Result(0));

//# programmable --sender A --dev-inspect
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::delete(Result(0));
//> 3: test::m::use_mut<test::m::Inner>(Result(1));
