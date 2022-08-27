// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module entry_point_vector::entry_point_vector {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;

    struct Obj has key {
        id: UID,
        value: u64
    }

    public entry fun mint(v: u64, ctx: &mut TxContext) {
        transfer::transfer(
            Obj {
                id: object::new(ctx), 
                value: v,
            },
            tx_context::sender(ctx),
        )
    }

    public entry fun prim_vec_len(v: vector<u64>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
    }

    public entry fun obj_vec_empty(v: vector<Obj>, _: &mut TxContext) {
        vector::destroy_empty(v);
    }

    public entry fun obj_vec_destroy(v: vector<Obj>, _: &mut TxContext) {
        assert!(vector::length(&v) == 2, 0);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 42, 0);
        object::delete(id);
        let Obj {id, value} = vector::pop_back(&mut v);
        assert!(value == 7, 0);
        object::delete(id);
        vector::destroy_empty(v);
    }


}
