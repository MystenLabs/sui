// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun inner(): Inner { Inner { f: 0, g: 0 } }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun take(_: u64) {}
public fun take_inner(_: Inner) {}
public fun copy_mut(_: u64, _: &mut u64) {}
public fun mut_copy(_: &mut u64, _: u64) {}

//# programmable --inputs 0 7
// VALID: copy first, then `&mut`, in one call
//> 0: test::m::copy_mut(Input(0), Input(0));

//# programmable --inputs 0 7
// VALID: `&mut` first, then copy, in one call
//> 0: test::m::mut_copy(Input(0), Input(0));

//# programmable --inputs 0 7
// VALID: copy of a pure input while a `&mut` result into it is live, later use sees the write
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::take(Input(0));
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));

//# programmable --inputs 7
// VALID: the last copy of a borrowed value stays a copy, the reference is still usable after
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::take(Input(0));
//> 2: test::m::write(Result(0), Input(0));

//# programmable --inputs 7
// VALID: copy of a struct result while a reference into its field is live
//> 0: test::m::inner();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take_inner(Result(0));
//> 3: test::m::write(Result(1), Input(0));
//> 4: test::m::check_inner(Result(0), Input(0));
