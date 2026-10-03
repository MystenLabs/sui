// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }

public fun check_nested(v: &vector<vector<u64>>, i: u64, j: u64, e: u64) { assert!(v[i][j] == e, 0) }

//# programmable --inputs 1 2 0 9
// VALID: the inner vector pushed once its element reference is dead, both writes visible
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: MakeMoveVec<vector<u64>>([Result(0)]);
//> 2: std::vector::borrow_mut<vector<u64>>(Result(1), Input(2));
//> 3: std::vector::borrow_mut<u64>(Result(2), Input(2));
//> 4: test::m::write(Result(3), Input(3));
//> 5: std::vector::push_back<u64>(Result(2), Input(0));
//> 6: test::m::check_nested(Result(1), Input(2), Input(2), Input(3));
//> 7: test::m::check_nested(Result(1), Input(2), Input(1), Input(0));
