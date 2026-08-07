// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Per-command TxContext rooting must not weaken borrow tracking for the other arguments: a
// `&mut Y` returned from `borrow_mut(&mut X, &mut TxContext)` still roots in the object,
// so mutably using the object while the result is live is rejected.

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

public fun touch(_x: &mut X, _ctx: &mut TxContext) {
}

public fun write(y: &mut Y, _ctx: &mut TxContext) {
    y.f = y.f + 1;
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable
// mutably using the object while the `&mut Y` result is live must fail
//> 0: test::m::new();
//> 1: test::m::borrow_mut(Result(0));
//> 2: test::m::touch(Result(0));
//> 3: test::m::write(Result(1));
//> 4: test::m::delete(Result(0));
