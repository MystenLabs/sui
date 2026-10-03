// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom(): &mut u64 { abort 7 }

public fun two_mut(_: &mut u64, _: &mut u64) {}

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 1, the same free-floating reference twice
//> 0: test::m::boom();
//> 1: test::m::two_mut(Result(0), Result(0));
