// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Same-call conflict detection with the mutable user argument *after* the injected
// TxContext: passing the same object as `&X` and `&mut X` around an injected
// `&mut TxContext` is rejected. This pins the reported argument index: the checker
// reports positions over the runtime argument list including the injected TxContext,
// so the user's second argument is reported as index 2, not 1.

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

public fun imm_ctx_mut(_a: &X, _ctx: &mut TxContext, _b: &mut X) {
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable
// the same object as `&X` and `&mut X` in one call must fail, ctx injected between them
//> 0: test::m::new();
//> 1: test::m::imm_ctx_mut(Result(0), Result(0));
//> 2: test::m::delete(Result(0));
