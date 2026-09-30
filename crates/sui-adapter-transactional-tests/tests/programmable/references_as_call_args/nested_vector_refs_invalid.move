// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }

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
