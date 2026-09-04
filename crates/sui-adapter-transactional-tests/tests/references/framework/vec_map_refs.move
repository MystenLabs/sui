// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }

//# programmable --inputs 1 2 9 0
// VALID: `get_mut` with a borrowed pure key, `get_entry_by_idx_mut` yielding a (key, value) reference pair
//> 0: sui::vec_map::empty<u64, u64>();
//> 1: sui::vec_map::insert<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::vec_map::get_mut<u64, u64>(Result(0), Input(0));
//> 3: test::m::write(Result(2), Input(2));
//> 4: sui::vec_map::get_entry_by_idx_mut<u64, u64>(Result(0), Input(3));
//> 5: test::m::check(NestedResult(4,0), Input(0));
//> 6: test::m::check(NestedResult(4,1), Input(2));
//> 7: test::m::write(NestedResult(4,1), Input(1));
//> 8: sui::vec_map::get<u64, u64>(Result(0), Input(0));
//> 9: test::m::check(Result(8), Input(1));

//# programmable --inputs 1 2 3 9
// INVALID: InvalidReferenceArgument at arg 0 of command 3, insert while a value reference is live
//> 0: sui::vec_map::empty<u64, u64>();
//> 1: sui::vec_map::insert<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::vec_map::get_mut<u64, u64>(Result(0), Input(0));
//> 3: sui::vec_map::insert<u64, u64>(Result(0), Input(2), Input(1));
//> 4: test::m::write(Result(2), Input(3));

//# programmable --inputs 1 2 0
// INVALID: InvalidReferenceArgument at arg 0 of command 3, remove while the entry pair is live
//> 0: sui::vec_map::empty<u64, u64>();
//> 1: sui::vec_map::insert<u64, u64>(Result(0), Input(0), Input(1));
//> 2: sui::vec_map::get_entry_by_idx_mut<u64, u64>(Result(0), Input(2));
//> 3: sui::vec_map::remove<u64, u64>(Result(0), Input(0));
//> 4: test::m::check(NestedResult(2,0), Input(0));
