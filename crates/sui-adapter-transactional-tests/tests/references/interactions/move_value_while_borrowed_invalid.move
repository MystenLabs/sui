// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { f: u64 }
public struct Outer has drop { nc: NoCopy }

public fun nc(): NoCopy { NoCopy { f: 0 } }
public fun outer(): Outer { Outer { nc: NoCopy { f: 0 } } }
public fun nc_mut(o: &mut Outer): &mut NoCopy { &mut o.nc }
public fun f(n: &NoCopy): &u64 { &n.f }
public fun f_mut(n: &mut NoCopy): &mut u64 { &mut n.f }
public fun take(_: NoCopy) {}
public fun take_outer(_: Outer) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, `&mut` child live
//> 0: test::m::nc();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::write(Result(1), Input(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, `&` child live
//> 0: test::m::nc();
//> 1: test::m::f(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::use_imm<u64>(Result(1));

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, grandchild live
//> 0: test::m::outer();
//> 1: test::m::nc_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::take_outer(Result(0));
//> 4: test::m::write(Result(2), Input(0));

//# programmable --inputs 1
// VALID: moved once the reference is dead
//> 0: test::m::nc();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(0));
//> 3: test::m::take(Result(0));
