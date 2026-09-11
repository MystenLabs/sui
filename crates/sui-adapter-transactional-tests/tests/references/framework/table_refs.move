// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_val(x: u64, v: u64) { assert!(x == v, 0) }

//# programmable --inputs 1 2 9
// VALID: write through `borrow_mut`, remove after the reference is dead
//> 0: sui::table::new<u64, u64>();
//> 1: sui::table::add<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::table::borrow_mut<u64, u64>(Result(0), Input(0));
//> 3: test::m::write(Result(2), Input(2));
//> 4: sui::table::remove<u64, u64>(Result(0), Input(0));
//> 5: test::m::check_val(Result(4), Input(2));
//> 6: sui::table::destroy_empty<u64, u64>(Result(0));

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

//# programmable --inputs 1 2
// VALID: two immutable references, then the table dropped once they are dead
//> 0: sui::table::new<u64, u64>();
//> 1: sui::table::add<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::table::borrow<u64, u64>(Result(0), Input(0));
//> 3: sui::table::borrow<u64, u64>(Result(0), Input(0));
//> 4: test::m::check(Result(2), Input(1));
//> 5: test::m::check(Result(3), Input(1));
//> 6: sui::table::drop<u64, u64>(Result(0));

//# programmable --inputs 1 2
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, the table consumed while borrowed
//> 0: sui::table::new<u64, u64>();
//> 1: sui::table::add<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::table::borrow<u64, u64>(Result(0), Input(0));
//> 3: sui::table::drop<u64, u64>(Result(0));
//> 4: test::m::check(Result(2), Input(1));
