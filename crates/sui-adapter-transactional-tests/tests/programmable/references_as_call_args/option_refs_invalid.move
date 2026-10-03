// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# programmable --inputs 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, extract while a reference is live
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow_mut<u64>(Result(0));
//> 2: std::option::extract<u64>(Result(0));
//> 3: test::m::write(Result(1), Input(1));

//# programmable --inputs 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, swap while an immutable reference is live
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::swap<u64>(Result(0), Input(1));
//> 3: test::m::check(Result(1), Input(0));
