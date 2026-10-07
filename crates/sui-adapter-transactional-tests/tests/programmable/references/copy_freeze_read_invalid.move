// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The last by-value use of a borrowed value is a Copy, not a Move, so a value with `copy` but
// not `drop` is left behind in its result slot.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct NoDrop has copy { f: u64 }

public fun no_drop(): NoDrop { NoDrop { f: 0 } }
public fun f_mut(x: &mut NoDrop): &mut u64 { &mut x.f }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun take(x: NoDrop) { let NoDrop { f: _ } = x; }

//# programmable --inputs 7
// INVALID: UnusedValueWithoutDrop { result_idx: 0, secondary_idx: 0 }, the last by-value use stays a copy while the value is borrowed
//> 0: test::m::no_drop();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::write(Result(1), Input(0));
