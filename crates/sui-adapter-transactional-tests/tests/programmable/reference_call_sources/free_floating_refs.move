// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom(): &mut u64 { abort 7 }
public fun launder(_: &u64): &mut u64 { abort 8 }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun write(r: &mut u64, v: u64) { *r = v }

//# programmable
// VALID: two free-floating references passed together, aborts at runtime with code 7
//> 0: test::m::boom();
//> 1: test::m::boom();
//> 2: test::m::two_mut(Result(0), Result(1));

//# programmable
// INVALID: InvalidReferenceArgument at arg 1 of command 1, the same free-floating reference twice
//> 0: test::m::boom();
//> 1: test::m::two_mut(Result(0), Result(0));

//# programmable --inputs 0 1
// VALID: a laundered reference does not borrow its immutable source, aborts at runtime with code 8
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::two_mut(Result(0), Result(1));

//# programmable --inputs 0 1
// VALID: writing the source while the laundered reference is live, aborts at runtime with code 8
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::write(Result(0), Input(1));
