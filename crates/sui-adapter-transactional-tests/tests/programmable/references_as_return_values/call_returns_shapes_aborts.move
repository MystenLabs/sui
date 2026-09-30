// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom_mut(): &mut u64 { abort 7 }
public fun boom_imm(): &u64 { abort 8 }
public fun launder(_: &u64): &mut u64 { abort 9 }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}

//# programmable --inputs 1
// ABORTS: static checking accepts a mutable reference without sources, but boom_mut aborts with code 7
//> 0: test::m::boom_mut();
//> 1: test::m::write(Result(0), Input(0));

//# programmable
// ABORTS: static checking accepts an immutable reference without sources, but boom_imm aborts with code 8
//> 0: test::m::boom_imm();
//> 1: test::m::use_imm<u64>(Result(0));

//# programmable --inputs 1 2
// ABORTS: static checking accepts a mutable reference from immutable arguments, but launder aborts with code 9
//> 0: test::m::launder(Input(0));
//> 1: test::m::write(Result(0), Input(1));
