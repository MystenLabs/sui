// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# programmable --inputs 1 2 9
// INVALID: InvalidReferenceArgument at arg 0 of command 3, remove while a reference is live
//> 0: sui::table::new<u64, u64>();
//> 1: sui::table::add<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::table::borrow_mut<u64, u64>(Result(0), Input(0));
//> 3: sui::table::remove<u64, u64>(Result(0), Input(0));
//> 4: test::m::write(Result(2), Input(2));
//> 5: sui::table::destroy_empty<u64, u64>(Result(0));

//# programmable --inputs 1 2 3
// INVALID: InvalidReferenceArgument at arg 0 of command 3, add while an immutable reference is live
//> 0: sui::table::new<u64, u64>();
//> 1: sui::table::add<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::table::borrow<u64, u64>(Result(0), Input(0));
//> 3: sui::table::add<u64, u64>(Result(0), Input(2), Input(1));
//> 4: test::m::check(Result(2), Input(1));
//> 5: sui::table::drop<u64, u64>(Result(0));
