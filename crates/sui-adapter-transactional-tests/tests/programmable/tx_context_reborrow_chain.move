// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, a chain of reborrows through consecutive ctx-taking
// calls (`&mut X` -> `&mut Y` -> `&mut u64`) roots transitively in the object only. The
// innermost reference stays valid across a later mutable TxContext use and is written
// through alongside a fresh `&mut TxContext` injection.

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

public fun borrow_inner(y: &mut Y, _ctx: &mut TxContext): &mut u64 {
    &mut y.f
}

public fun write_inner(f: &mut u64, _ctx: &mut TxContext) {
    *f = *f + 1;
}

public fun mut_tx(_: &mut TxContext) {
}

public fun check_inner(f: &u64, expected: u64, _ctx: &TxContext) {
    assert!(*f == expected, 0);
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable --inputs 1
//> 0: test::m::new();
//> 1: test::m::borrow_mut(Result(0));
//> 2: test::m::borrow_inner(Result(1));
//> 3: test::m::mut_tx();
//> 4: test::m::write_inner(Result(2));
//> 5: test::m::check_inner(Result(2), Input(0));
//> 6: test::m::delete(Result(0));
