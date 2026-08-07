// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Flag-off twin of tx_context_result_across_mut_tx_use. Dev-inspect permits reference
// return values even without --enable-feature-flags allow_references_in_ptbs, but without the flag every
// injected TxContext shares a single borrow root, so the `&mut Y` result (which extends
// that root) conflicts with the later `&mut TxContext` injection in `mut_tx`.

//# init --addresses test=0x0 --accounts A

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

public fun mut_tx(_: &mut TxContext) {
}

public fun check(y: &Y, expected: u64, _ctx: &TxContext) {
    assert!(y.f == expected, 0);
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable --sender A --dev-inspect --inputs 1
//> 0: test::m::new();
//> 1: test::m::borrow_mut(Result(0));
//> 2: test::m::mut_tx();
//> 3: test::m::write(Result(1));
//> 4: test::m::check(Result(1), Input(0));
//> 5: test::m::delete(Result(0));
