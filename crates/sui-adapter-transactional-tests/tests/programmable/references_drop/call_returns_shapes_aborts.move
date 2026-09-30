// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom_mut(): &mut u64 { abort 7 }

//# programmable
// ABORTS: an unused free-floating result still runs the call, aborts at runtime with code 7
//> 0: test::m::boom_mut();
