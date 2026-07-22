// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Under --enable-feature-flags allow_references_in_ptbs, a purely TxContext-derived reference
// (`digest(&TxContext): &vector<u8>`) and an object-rooted `&mut Y` coexist across
// interleaved mutable TxContext uses, and both remain usable at the end.

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

public fun mut_tx(_: &mut TxContext) {
}

public fun eq_digests(a: &vector<u8>, b: &vector<u8>) {
    assert!(*a == *b, 0);
}

public fun check(y: &Y, expected: u64, _ctx: &TxContext) {
    assert!(y.f == expected, 0);
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable --inputs 1
//> 0: sui::tx_context::digest();
//> 1: test::m::new();
//> 2: test::m::borrow_mut(Result(1));
//> 3: test::m::mut_tx();
//> 4: test::m::write(Result(2));
//> 5: sui::tx_context::digest();
//> 6: test::m::eq_digests(Result(0), Result(5));
//> 7: test::m::check(Result(2), Input(0));
//> 8: test::m::delete(Result(1));
