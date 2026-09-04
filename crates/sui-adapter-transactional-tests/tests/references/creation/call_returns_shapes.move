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
public fun inner_frozen(o: &mut Obj): &Inner { &o.inner }
public fun boom_mut(): &mut u64 { abort 7 }
public fun boom_imm(): &u64 { abort 8 }
public fun launder(_: &u64): &mut u64 { abort 9 }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# programmable
// VALID: `&Inner` from `&mut Obj`
//> 0: test::m::new();
//> 1: test::m::inner_frozen(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable
// VALID: `&Inner` from `&Obj`
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::delete(Result(0));

//# programmable --inputs 1
// VALID: `&mut u64` with no reference arguments, aborts at runtime with code 7
//> 0: test::m::boom_mut();
//> 1: test::m::write(Result(0), Input(0));

//# programmable
// VALID: `&u64` with no arguments, aborts at runtime with code 8
//> 0: test::m::boom_imm();
//> 1: test::m::use_imm<u64>(Result(0));

//# programmable --inputs 1 2
// VALID: `&mut u64` from only immutable arguments, aborts at runtime with code 9
//> 0: test::m::launder(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# programmable
// VALID: an unused free-floating result still runs the call, aborts at runtime with code 7
//> 0: test::m::boom_mut();
