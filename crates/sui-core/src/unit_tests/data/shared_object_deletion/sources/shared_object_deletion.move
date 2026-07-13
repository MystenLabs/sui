// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module shared_object_deletion::o2;

use std::vector;
use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};

public struct Obj has key, store {
    id: UID,
    flipped: bool,
}

public struct Obj2 has key, store {
    id: UID,
    mutation_count: u64,
}

public struct Wrapper has key {
    id: UID,
    o2: Obj2,
}

public fun create_owned(ctx: &mut TxContext) {
    let o = Obj { id: object::new(ctx), flipped: false };
    let sender = ctx.sender();
    transfer::transfer(o, sender)
}

public fun create(ctx: &mut TxContext) {
    let o = Obj2 { id: object::new(ctx), mutation_count: 0 };
    transfer::public_share_object(o)
}

public fun consume_o2(o2: Obj2) {
    let Obj2 { id, mutation_count: _mutation_count } = o2;
    id.delete();
}

public fun consume_with_owned(o: &mut Obj, o2: Obj2) {
    let Obj2 { id, mutation_count: _mutation_count } = o2;
    id.delete();
    o.flipped = true;
}

public fun consume_with_shared(o: &mut Obj2, o2: Obj2) {
    if (o.mutation_count < o2.mutation_count) {
        let Obj2 { id, mutation_count: _mutation_count } = o2;
        id.delete();
    } else {
        re_share_o2(o2)
    };
    mutate_o2(o);
}

public fun mutate_o2(o2: &mut Obj2) {
    let m = o2.mutation_count + 1;
    o2.mutation_count = m;
}

public fun mutate_o2_with_shared(o: &mut Obj2, o2: &mut Obj2) {
    let n = o.mutation_count + 1;
    o.mutation_count = n;
    let m = o2.mutation_count + 1;
    o2.mutation_count = m;
}

public fun mutate_with_owned(o: &mut Obj, o2: &mut Obj2) {
    let m = o2.mutation_count + 1;
    o2.mutation_count = m;

    o.flipped = !o.flipped;
}

public fun freeze_o2(o2: Obj2) {
    transfer::freeze_object(o2);
}

public fun transfer_to_single_owner(o2: Obj2, ctx: &mut TxContext) {
    transfer::transfer(o2, ctx.sender())
}

public fun re_share_o2(o2: Obj2) {
    transfer::public_share_object(o2)
}

public fun re_share_non_public_o2(o2: Obj2) {
    transfer::share_object(o2)
}

public fun wrap_o2(o2: Obj2, ctx: &mut TxContext) {
    transfer::transfer(Wrapper { id: object::new(ctx), o2 }, ctx.sender())
}

public fun vec_delete(mut v: vector<Obj2>) {
    let Obj2 { id, mutation_count: _ } = v.pop_back();
    id.delete();
    v.destroy_empty();
}

public fun read_o2(_o2: &Obj2) {}

public fun read_and_read(_o1: &Obj2, _o2: &Obj2) {}

public fun read_and_write(_o1: &Obj2, _o2: &mut Obj2) {}

public fun mutate_and_mutate(_o1: &mut Obj2, _o2: &mut Obj2) {}
