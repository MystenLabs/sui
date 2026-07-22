// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, a `&Y` returned from `borrow(&X, &TxContext)` roots
// only in the object, not in the injected TxContext. The immutable result stays valid
// across a later command that mutably uses the TxContext, and is then read alongside a
// fresh `&TxContext` injection.

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
    X { id: object::new(ctx), y: Y { f: 42 } }
}

public fun borrow(x: &X, _ctx: &TxContext): &Y {
    &x.y
}

public fun mut_tx(_: &mut TxContext) {
}

public fun check(y: &Y, expected: u64, _ctx: &TxContext) {
    assert!(y.f == expected, 0);
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable --inputs 42
//> 0: test::m::new();
//> 1: test::m::borrow(Result(0));
//> 2: test::m::mut_tx();
//> 3: test::m::check(Result(1), Input(0));
//> 4: test::m::delete(Result(0));
