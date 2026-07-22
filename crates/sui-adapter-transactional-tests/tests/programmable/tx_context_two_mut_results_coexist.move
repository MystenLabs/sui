// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, two `&mut Y` results from two calls that each take
// `&mut TxContext` can coexist and both be written through. TxContext borrows root per
// borrowing command, so the two results extend distinct TxContext roots; with a single
// shared root the second `borrow_mut` (and every use after it) would be rejected.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct X has key, store {
    id: UID,
    y: Y,
}

public struct Y has store {
    f: u64,
}

public fun new(ctx: &mut TxContext): X {
    X { id: object::new(ctx), y: Y { f: 0 } }
}

public fun borrow_mut(x: &mut X, _ctx: &mut TxContext): &mut Y {
    &mut x.y
}

public fun write(y: &mut Y, _ctx: &mut TxContext) {
    y.f = y.f + 1;
}

public fun check(y: &Y, expected: u64, _ctx: &TxContext) {
    assert!(y.f == expected, 0);
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable --inputs 1
//> 0: test::m::new();
//> 1: test::m::new();
//> 2: test::m::borrow_mut(Result(0));
//> 3: test::m::borrow_mut(Result(1));
//> 4: test::m::write(Result(2));
//> 5: test::m::write(Result(3));
//> 6: test::m::check(Result(2), Input(0));
//> 7: test::m::check(Result(3), Input(0));
//> 8: test::m::delete(Result(0));
//> 9: test::m::delete(Result(1));
