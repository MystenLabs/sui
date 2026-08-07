// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Per-command TxContext rooting must not weaken same-call conflict detection when the
// TxContext parameter sits between two conflicting user arguments: passing the same
// object as both `&mut X` and `&X` around an injected `&mut TxContext` is rejected.
// This also pins the reported argument index: the checker reports positions over the
// runtime argument list (which includes the injected TxContext), not the user's
// argument list.

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

public fun mut_ctx_imm(_a: &mut X, _ctx: &mut TxContext, _b: &X) {
}

public fun delete(x: X) {
    let X { id, y: Y { f: _ } } = x;
    object::delete(id);
}

//# programmable
// the same object as `&mut X` and `&X` in one call must fail, ctx injected between them
//> 0: test::m::new();
//> 1: test::m::mut_ctx_imm(Result(0), Result(0));
//> 2: test::m::delete(Result(0));
