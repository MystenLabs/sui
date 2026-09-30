// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_val(x: u64, v: u64) { assert!(x == v, 0) }

//# programmable --inputs 1 9
// VALID: write through `borrow_mut`, then extract after the reference is dead
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow_mut<u64>(Result(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: std::option::extract<u64>(Result(0));
//> 4: test::m::check_val(Result(3), Input(1));

//# programmable --inputs 1
// VALID: two immutable borrows coexist
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::borrow<u64>(Result(0));
//> 3: test::m::check(Result(1), Input(0));
//> 4: test::m::check(Result(2), Input(0));
