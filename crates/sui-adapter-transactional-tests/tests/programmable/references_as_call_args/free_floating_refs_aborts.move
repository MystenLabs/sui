// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom(): &mut u64 { abort 7 }

public fun two_mut(_: &mut u64, _: &mut u64) {}

//# programmable
// ABORTS: static checking permits two distinct free-floating mutable arguments, but boom aborts with code 7 before the call
//> 0: test::m::boom();
//> 1: test::m::boom();
//> 2: test::m::two_mut(Result(0), Result(1));
