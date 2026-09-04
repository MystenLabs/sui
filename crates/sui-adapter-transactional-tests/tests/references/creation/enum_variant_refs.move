// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public enum X has copy, drop { One { x: u64 }, Two { x: u64, y: u64 } }

public fun make_one(x: u64): X { X::One { x } }
public fun make_two(x: u64, y: u64): X { X::Two { x, y } }
public fun unpack_one_mut(e: &mut X): &mut u64 {
    match (e) { X::One { x } => x, _ => abort 0 }
}
public fun unpack_two_mut(e: &mut X): (&mut u64, &mut u64) {
    match (e) { X::Two { x, y } => (x, y), _ => abort 0 }
}
public fun unpack_two(e: &X): (&u64, &u64) {
    match (e) { X::Two { x, y } => (x, y), _ => abort 0 }
}
public fun set_one(e: &mut X, x: u64) { *e = X::One { x } }
public fun set_two(e: &mut X, x: u64, y: u64) { *e = X::Two { x, y } }
public fun check_one(e: &X, x: u64) {
    match (e) { X::One { x: v } => assert!(*v == x, 1), _ => abort 2 }
}
public fun check_two(e: &X, x: u64, y: u64) {
    match (e) { X::Two { x: a, y: b } => assert!(*a == x && *b == y, 3), _ => abort 4 }
}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun read(r: &u64): u64 { *r }
public fun check(r: &u64, v: u64) { assert!(*r == v, 5) }
public fun check_eq(a: u64, b: u64) { assert!(a == b, 6) }

//# programmable --inputs 0 1 2
// VALID: the variant field reference is dead before the enum is rewritten as the other variant and unpacked again
//> 0: test::m::make_one(Input(0));
//> 1: test::m::unpack_one_mut(Result(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::check_one(Result(0), Input(1));
//> 4: test::m::set_two(Result(0), Input(1), Input(0));
//> 5: test::m::unpack_two_mut(Result(0));
//> 6: test::m::write(NestedResult(5, 0), Input(2));
//> 7: test::m::check_two(Result(0), Input(2), Input(0));

//# programmable --inputs 7 8 0
// INVALID: InvalidReferenceArgument at arg 0 of command 2, the enum is rewritten while an immutable variant field reference is live
//> 0: test::m::make_two(Input(0), Input(1));
//> 1: test::m::unpack_two(Result(0));
//> 2: test::m::set_one(Result(0), Input(2));
//> 3: test::m::check(NestedResult(1, 1), Input(1));
