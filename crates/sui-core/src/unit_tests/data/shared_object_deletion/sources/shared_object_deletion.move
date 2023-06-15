// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module shared_object_deletion::o2 {
    use std::vector;
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct Obj2 has key, store {
        id: UID,
        mutation_count: u64,
    }

    struct Wrapper has key {
        id: UID,
        o2: Obj2
    }

    public entry fun create(ctx: &mut TxContext) {
        let o = Obj2 { id: object::new(ctx), mutation_count: 0 };
        transfer::public_share_object(o)
    }

    public entry fun consume_o2(o2: Obj2) {
        let Obj2 { id, mutation_count: _mutation_count } = o2;
        object::delete(id);
    }

    public entry fun mutate_o2(o2:  &mut Obj2) {
        let m = o2.mutation_count + 1;
        o2.mutation_count = m;
    }

    public entry fun freeze_o2(o2: Obj2) {
        transfer::freeze_object(o2);
    }

    public entry fun transfer_to_single_owner(o2: Obj2, ctx: &mut TxContext) {
        transfer::transfer(o2, tx_context::sender(ctx))
    }

    public entry fun re_share_o2(o2: Obj2) {
        transfer::public_share_object(o2)
    }

    public entry fun re_share_non_public_o2(o2: Obj2) {
        transfer::share_object(o2)
    }

    public entry fun wrap_o2(o2: Obj2, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), o2}, tx_context::sender(ctx))
    }

    public entry fun vec_delete(v: vector<Obj2>) {
        let Obj2 {id, mutation_count: _} = vector::pop_back(&mut v);
        object::delete(id);
        vector::destroy_empty(v);
    }

}