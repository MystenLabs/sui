// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A returned `&mut TxContext` is unusable but must stay out of the way: it does not
// conflict with later injections and is released cleanly.

//# init --addresses test=0x0 --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun mut_id(ctx: &mut TxContext): &mut TxContext {
    ctx
}

public fun mut_tx(_: &mut TxContext) {
}

public fun eq_digests(a: &vector<u8>, b: &vector<u8>) {
    assert!(*a == *b, 0);
}

//# programmable
//> 0: test::m::mut_id();
//> 1: test::m::mut_tx();
//> 2: sui::tx_context::digest();
//> 3: sui::tx_context::digest();
//> 4: test::m::eq_digests(Result(2), Result(3));
