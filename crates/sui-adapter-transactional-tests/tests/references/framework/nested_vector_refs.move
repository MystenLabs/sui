// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun check_nested(v: &vector<vector<u64>>, i: u64, j: u64, e: u64) { assert!(v[i][j] == e, 0) }

//# programmable --inputs 1 2 0 9
// VALID: the nested vector holds a copy taken before the write, the original holds the write
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: MakeMoveVec<vector<u64>>([Result(0)]);
//> 3: test::m::write(Result(1), Input(3));
//> 4: test::m::check_nested(Result(2), Input(2), Input(2), Input(0));
//> 5: test::m::check_elem(Result(0), Input(2), Input(3));

//# programmable --inputs 1 2 0 9
// VALID: a depth-two write through `&mut vector<u64>` then `&mut u64`
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: MakeMoveVec<vector<u64>>([Result(0)]);
//> 2: std::vector::borrow_mut<vector<u64>>(Result(1), Input(2));
//> 3: std::vector::borrow_mut<u64>(Result(2), Input(2));
//> 4: test::m::write(Result(3), Input(3));
//> 5: test::m::check_nested(Result(1), Input(2), Input(2), Input(3));

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 4, outer vector pushed while the depth-two element is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: MakeMoveVec<vector<u64>>([Result(0)]);
//> 2: std::vector::borrow_mut<vector<u64>>(Result(1), Input(2));
//> 3: std::vector::borrow_mut<u64>(Result(2), Input(2));
//> 4: std::vector::push_back<vector<u64>>(Result(1), Result(0));
//> 5: test::m::write(Result(3), Input(3));

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 4, inner vector pushed while its element is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: MakeMoveVec<vector<u64>>([Result(0)]);
//> 2: std::vector::borrow_mut<vector<u64>>(Result(1), Input(2));
//> 3: std::vector::borrow_mut<u64>(Result(2), Input(2));
//> 4: std::vector::push_back<u64>(Result(2), Input(0));
//> 5: test::m::write(Result(3), Input(3));

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
