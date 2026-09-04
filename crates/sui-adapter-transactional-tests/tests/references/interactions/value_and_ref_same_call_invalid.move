// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoCopy has drop { f: u64 }

public fun nc(): NoCopy { NoCopy { f: 0 } }
public fun f_mut(n: &mut NoCopy): &mut u64 { &mut n.f }
public fun take_and_imm(_: NoCopy, _: &NoCopy) { abort 0 }
public fun imm_and_take(_: &NoCopy, _: NoCopy) { abort 0 }
public fun take_and_mut(_: NoCopy, _: &mut NoCopy) { abort 0 }
public fun mut_and_take(_: &mut NoCopy, _: NoCopy) { abort 0 }
public fun take_and_u64(_: NoCopy, _: &mut u64) { abort 0 }
public fun u64_and_take(_: &mut u64, _: NoCopy) { abort 0 }

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::take_and_imm(Result(0), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::imm_and_take(Result(0), Result(0));

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::take_and_mut(Result(0), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1
//> 0: test::m::nc();
//> 1: test::m::mut_and_take(Result(0), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, the value moved with its own field reference
//> 0: test::m::nc();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take_and_u64(Result(0), Result(1));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, the field reference first
//> 0: test::m::nc();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::u64_and_take(Result(1), Result(0));
