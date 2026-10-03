// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun launder(_: &u64): &mut u64 { abort 8 }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable --inputs 0 1
// ABORTS: static checking permits mutable arguments from an immutable source and its mutable result, but launder aborts with code 8
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::two_mut(Result(0), Result(1));

//# programmable --inputs 0 1
// ABORTS: static checking permits writing the immutable source with its mutable result live, but launder aborts with code 8
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::write(Result(0), Input(1));
