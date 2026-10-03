// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { f: u64 }

public fun nc(): NoCopy { NoCopy { f: 0 } }

public fun f_mut(n: &mut NoCopy): &mut u64 { &mut n.f }
public fun take(_: NoCopy) {}

public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable --inputs 1
// VALID: moved once the reference is dead
//> 0: test::m::nc();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(0));
//> 3: test::m::take(Result(0));
