// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Inner has copy, drop { f: u64 }

public fun inner_val(): Inner { Inner { f: 0 } }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun take_inner(i: Inner, f: u64) { assert!(i.f == f, 0) }

//# programmable --inputs 7 0
// VALID: a by-value read observes the parent while its mutable child remains writable
//> 0: test::m::inner_val();
//> 1: test::m::id_mut<test::m::Inner>(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::take_inner(Result(1), Input(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
