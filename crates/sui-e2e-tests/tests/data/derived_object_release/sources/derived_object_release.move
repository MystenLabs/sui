// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Each claim of the derived object checks and bumps a counter kept in a dynamic field of the
/// object, which survives every release.
module derived_object_release::m;

use sui::derived_object;
use sui::dynamic_field as df;
use sui::party;

const EUnexpectedCount: u64 = 0;

public struct Parent has key { id: UID }

public struct Obj has key, store { id: UID }

public struct Wrapper has key { id: UID, inner: Obj }

public fun create_parent(ctx: &mut TxContext) {
    transfer::transfer(Parent { id: object::new(ctx) }, ctx.sender())
}

public fun claim_owned(parent: &mut Parent, expected: u64, ctx: &TxContext) {
    transfer::transfer(claim(parent, expected), ctx.sender())
}

public fun claim_party(parent: &mut Parent, expected: u64, ctx: &TxContext) {
    transfer::party_transfer(claim(parent, expected), party::single_owner(ctx.sender()))
}

public fun claim_wrapped(parent: &mut Parent, expected: u64, ctx: &mut TxContext) {
    let inner = claim(parent, expected);
    transfer::transfer(Wrapper { id: object::new(ctx), inner }, ctx.sender())
}

public fun release(parent: &mut Parent, obj: Obj) {
    let Obj { id } = obj;
    derived_object::release(&mut parent.id, id)
}

public fun release_wrapped(parent: &mut Parent, wrapper: Wrapper) {
    let Wrapper { id, inner } = wrapper;
    id.delete();
    release(parent, inner)
}

fun claim(parent: &mut Parent, expected: u64): Obj {
    let mut obj = Obj { id: derived_object::claim(&mut parent.id, 0u64) };
    if (!df::exists(&obj.id, 0u64)) df::add(&mut obj.id, 0u64, 0u64);
    let count: &mut u64 = df::borrow_mut(&mut obj.id, 0u64);
    assert!(*count == expected, EUnexpectedCount);
    *count = *count + 1;
    obj
}
