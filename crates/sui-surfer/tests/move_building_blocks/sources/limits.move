// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_building_blocks::limits {
    use sui::object::UID;
    use sui::object;
    use sui::tx_context::TxContext;
    use std::vector;
    use sui::transfer;
    use sui::tx_context;
    use sui::dynamic_field;

    struct ObjectWithVector has key, store {
        id: UID,
        array: vector<u64>,
    }

    public fun create_object_with_size(size: u64, ctx: &mut TxContext) {
        let v = vector[];
        let i = 0;
        while (i < size) {
            vector::push_back(&mut v, i);
            i = i + 1;
        };
        let object = ObjectWithVector {
            id: object::new(ctx),
            array: v,
        };
        transfer::transfer(object, tx_context::sender(ctx));
    }

    public fun create_object_with_grand_children(depth: u64, ctx: &mut TxContext) {
        let object = create_object_recursive(depth, ctx);
        transfer::transfer(object, tx_context::sender(ctx));
    }

    fun create_object_recursive(depth: u64, ctx: &mut TxContext): ObjectWithVector {
        let object = ObjectWithVector {
            id: object::new(ctx),
            array: vector[],
        };
        if (depth > 0) {
            let child = create_object_recursive(depth - 1, ctx);
            dynamic_field::add(&mut object.id, depth, child);
        };
        object
    }
}
