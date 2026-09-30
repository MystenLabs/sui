// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public enum X has copy, drop { One { x: u64 }, Two { x: u64, y: u64 } }

public fun make_two(x: u64, y: u64): X { X::Two { x, y } }

public fun unpack_two(e: &X): (&u64, &u64) {
    match (e) { X::Two { x, y } => (x, y), _ => abort 0 }
}
public fun set_one(e: &mut X, x: u64) { *e = X::One { x } }

public fun check(r: &u64, v: u64) { assert!(*r == v, 5) }

//# programmable --inputs 7 8 0
// INVALID: InvalidReferenceArgument at arg 0 of command 2, the enum is rewritten while an immutable variant field reference is live
//> 0: test::m::make_two(Input(0), Input(1));
//> 1: test::m::unpack_two(Result(0));
//> 2: test::m::set_one(Result(0), Input(2));
//> 3: test::m::check(NestedResult(1, 1), Input(1));
