// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_val(x: u64, v: u64) { assert!(x == v, 0) }
public struct NoCopy has drop { v: u64 }
public fun nc(v: u64): NoCopy { NoCopy { v } }
public fun check_nc(r: &NoCopy, v: u64) { assert!(r.v == v, 0) }

//# programmable --inputs 1 9
// VALID: write through `borrow_mut`, then extract after the reference is dead
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow_mut<u64>(Result(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: std::option::extract<u64>(Result(0));
//> 4: test::m::check_val(Result(3), Input(1));

//# programmable --inputs 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, extract while a reference is live
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow_mut<u64>(Result(0));
//> 2: std::option::extract<u64>(Result(0));
//> 3: test::m::write(Result(1), Input(1));

//# programmable --inputs 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, fill/swap while an immutable reference is live
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::swap<u64>(Result(0), Input(1));
//> 3: test::m::check(Result(1), Input(0));

//# programmable --inputs 1
// VALID: two immutable borrows coexist
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::borrow<u64>(Result(0));
//> 3: test::m::check(Result(1), Input(0));
//> 4: test::m::check(Result(2), Input(0));

//# programmable --inputs 1
// VALID: a copyable option is copied, not moved, by `destroy_some` while borrowed
//> 0: std::option::some<u64>(Input(0));
//> 1: std::option::borrow<u64>(Result(0));
//> 2: std::option::destroy_some<u64>(Result(0));
//> 3: test::m::check(Result(1), Input(0));

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, a non-copy option consumed while borrowed
//> 0: test::m::nc(Input(0));
//> 1: std::option::some<test::m::NoCopy>(Result(0));
//> 2: std::option::borrow<test::m::NoCopy>(Result(1));
//> 3: std::option::destroy_some<test::m::NoCopy>(Result(1));
//> 4: test::m::check_nc(Result(2), Input(0));
