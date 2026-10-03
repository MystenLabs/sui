// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { f: u64 }

public fun nc(): NoCopy { NoCopy { f: 0 } }

public fun take_and_imm(_: NoCopy, _: &NoCopy) { abort 0 }

public fun take_and_mut(_: NoCopy, _: &mut NoCopy) { abort 0 }

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::take_and_imm(Result(0), Result(0));

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::take_and_mut(Result(0), Result(0));
