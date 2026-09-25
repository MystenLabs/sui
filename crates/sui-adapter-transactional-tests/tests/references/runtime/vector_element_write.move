// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun check_len(v: &vector<u64>, n: u64) { assert!(v.length() == n, 0) }

//# programmable --inputs 1 2 0 1 9
// VALID: write element 0 then element 1 through separate references, check both
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: test::m::write(Result(1), Input(4));
//> 3: std::vector::borrow_mut<u64>(Result(0), Input(3));
//> 4: test::m::write(Result(3), Input(4));
//> 5: test::m::check_elem(Result(0), Input(2), Input(4));
//> 6: test::m::check_elem(Result(0), Input(3), Input(4));

//# programmable --inputs 1 2 0 9 3
// VALID: push after the element reference is dead, length grows
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: test::m::write(Result(1), Input(3));
//> 3: std::vector::push_back<u64>(Result(0), Input(3));
//> 4: test::m::check_len(Result(0), Input(4));
