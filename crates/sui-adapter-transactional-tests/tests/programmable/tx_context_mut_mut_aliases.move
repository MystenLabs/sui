// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Demonstrates that passing `&mut TxContext` twice into the same Move call
// resolves to a single underlying TxContext value at runtime: both references
// observe the same digest. The aliasing-violation that Move's type system
// would normally object to is harmless here because no Move function ever
// mutates a TxContext field through `&mut TxContext`.

//# init --addresses test=0x0 --allow-references-in-ptbs

//# publish
module test::m;

use sui::tx_context::digest;

public fun mut_mut_observe(a: &mut TxContext, b: &mut TxContext) {
    let da = *digest(a);
    let db = *digest(b);
    assert!(da == db, 0);
}

//# programmable
//> test::m::mut_mut_observe();
