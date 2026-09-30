// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun write(r: &mut u64, v: u64) { *r = v }

public fun use_imm<T>(_: &T) {}

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, push_back while an element reference is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::push_back<u64>(Result(0), Input(3));
//> 3: test::m::write(Result(1), Input(3));

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, pop_back while an element reference is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::pop_back<u64>(Result(0));
//> 3: test::m::write(Result(1), Input(3));

//# programmable --inputs 1 2 0 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, swap while an element reference is live
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::swap<u64>(Result(0), Input(2), Input(3));
//> 3: test::m::write(Result(1), Input(4));

//# programmable --inputs 1 2 0 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, an immutable element reference also blocks push_back
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow<u64>(Result(0), Input(2));
//> 2: std::vector::push_back<u64>(Result(0), Input(3));
//> 3: test::m::use_imm<u64>(Result(1));

//# programmable --inputs 1 2 0 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 2, two mutable element borrows in two commands
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::borrow_mut<u64>(Result(0), Input(3));
//> 3: test::m::write(Result(1), Input(4));
//> 4: test::m::write(Result(2), Input(4));

//# programmable --inputs 1 2 0 1 9
// INVALID: InvalidReferenceArgument at arg 0 of command 3, a mutable then an immutable element borrow, then the mutable one written
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: std::vector::borrow<u64>(Result(0), Input(3));
//> 3: test::m::write(Result(1), Input(4));
//> 4: test::m::use_imm<u64>(Result(2));
